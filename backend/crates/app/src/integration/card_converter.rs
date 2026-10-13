//! Management, health monitoring and protocol calls for optional card converters.

use std::sync::Arc;
use std::time::Instant;

use regex::Regex;
use rust_decimal::Decimal;
use serde_json::{Map, Value};
use std::str::FromStr;
use zs_domain::integration::card_converter::{
    Binding, BindingDependencies, Converter, ConverterEvent, ConverterHttp, ConverterInput,
    ConverterRepo, ExchangeItem, ExchangeRequest, ExchangeResponse, NewConverterEvent,
};
use zs_domain::integration::hooks::UpstreamDelivery;
use zs_domain::notify::center::{NotifyEvent, events};
use zs_domain::notify::ports::Notifier;
use zs_domain::order::model::Order;
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::crypto::Cipher;

#[derive(Clone)]
pub struct CardConverterService {
    repo: Arc<dyn ConverterRepo>,
    http: Arc<dyn ConverterHttp>,
    cipher: Cipher,
    clock: Arc<dyn Clock>,
    notifier: Arc<dyn Notifier>,
}

impl std::fmt::Debug for CardConverterService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CardConverterService")
    }
}

impl CardConverterService {
    pub async fn has_binding(&self, order: &Order) -> Result<bool> {
        if order.items.len() != 1 {
            return Ok(false);
        }
        let item = &order.items[0];
        self.has_product_binding(item.product_id, item.sku_id).await
    }

    pub async fn has_product_binding(&self, product_id: Id, sku_id: Id) -> Result<bool> {
        Ok(self.repo.get_binding(product_id, sku_id).await?.is_some()
            || self.repo.get_binding(product_id, 0).await?.is_some())
    }

    pub fn new(
        repo: Arc<dyn ConverterRepo>,
        http: Arc<dyn ConverterHttp>,
        cipher: Cipher,
        clock: Arc<dyn Clock>,
        notifier: Arc<dyn Notifier>,
    ) -> Self {
        Self {
            repo,
            http,
            cipher,
            clock,
            notifier,
        }
    }

    pub async fn list(&self) -> Result<Vec<Converter>> {
        Ok(self.repo.list().await?.into_iter().map(mask).collect())
    }

    pub async fn get(&self, id: Id) -> Result<Converter> {
        self.repo.get(id).await?.map(mask).ok_or_else(not_found)
    }

    pub async fn save(&self, id: Id, input: ConverterInput) -> Result<Converter> {
        let name = input.name.trim();
        let base_url = normalize_url(&input.base_url)?;
        if name.is_empty() || name.chars().count() > 100 {
            return Err(invalid());
        }
        let (token_enc, types, created_at) = if id > 0 {
            let old = self.repo.get(id).await?.ok_or_else(not_found)?;
            let token_enc = if input.token.trim().is_empty() {
                old.token_enc.clone()
            } else {
                self.cipher
                    .encrypt(input.token.trim())
                    .map_err(Error::internal)?
            };
            (token_enc, old.types, old.created_at)
        } else {
            if input.token.trim().len() < 24 || input.token.len() > 512 {
                return Err(invalid());
            }
            (
                self.cipher
                    .encrypt(input.token.trim())
                    .map_err(Error::internal)?,
                Vec::new(),
                self.clock.now(),
            )
        };
        if token_enc.is_empty() {
            return Err(invalid());
        }
        let now = self.clock.now();
        let saved = self
            .repo
            .save(&Converter {
                id,
                name: name.to_owned(),
                base_url,
                token_enc,
                token_configured: true,
                enabled: input.enabled,
                types,
                health: "unknown".into(),
                consecutive_failures: 0,
                last_checked_at: None,
                last_error: String::new(),
                created_at,
                updated_at: now,
            })
            .await?;
        // The connection is configured independently of converter-specific categories.
        Ok(mask(saved))
    }

    pub async fn delete(&self, id: Id) -> Result<()> {
        if !self
            .repo
            .bindings(None)
            .await?
            .iter()
            .all(|b| b.converter_id != id)
        {
            return Err(Error::bad_request("error.card_converter_in_use"));
        }
        self.repo.delete(id).await
    }

    pub async fn test(&self, id: Id) -> Result<Value> {
        let converter = self.repo.get(id).await?.ok_or_else(not_found)?;
        let token = self
            .cipher
            .decrypt(&converter.token_enc)
            .map_err(Error::internal)?;
        let started = Instant::now();
        self.http.health(&converter.base_url, &token).await?;
        self.record_health(id, true, "", elapsed_ms(started))
            .await?;
        Ok(serde_json::json!({"ok":true}))
    }

    pub async fn bindings(&self, product_id: Option<Id>) -> Result<Vec<Binding>> {
        let mut bindings = self.repo.bindings(product_id).await?;
        for binding in &mut bindings {
            let dependencies = self.repo.binding_dependencies(binding).await?;
            binding.issues = binding_issues(&binding.extra_template, &dependencies);
        }
        Ok(bindings)
    }

    pub async fn uses_converter(
        &self,
        product_id: Id,
        sku_id: Id,
        converter_id: Id,
    ) -> Result<bool> {
        let binding = self
            .repo
            .get_binding(product_id, sku_id)
            .await?
            .or(self.repo.get_binding(product_id, 0).await?);
        Ok(binding.is_some_and(|binding| binding.converter_id == converter_id))
    }

    pub async fn save_binding(&self, mut binding: Binding) -> Result<Binding> {
        if binding.product_id <= 0 || binding.sku_id < 0 || binding.converter_id <= 0 {
            return Err(invalid());
        }
        let converter = self
            .repo
            .get(binding.converter_id)
            .await?
            .ok_or_else(not_found)?;
        if !converter.enabled {
            return Err(Error::bad_request("error.card_converter_binding_invalid"));
        }
        if !binding.extra_template.is_object() && !binding.extra_template.is_null() {
            return Err(invalid());
        }
        if !binding.extra_template.is_object() {
            binding.extra_template = Value::Object(Map::new());
        }
        validate_template(&binding.extra_template)?;
        self.repo.save_binding(&binding).await
    }

    pub async fn delete_binding(&self, product_id: Id, sku_id: Id) -> Result<()> {
        self.repo.delete_binding(product_id, sku_id).await
    }

    pub async fn events(&self, id: Id, limit: u64) -> Result<Vec<ConverterEvent>> {
        if self.repo.get(id).await?.is_none() {
            return Err(not_found());
        }
        self.repo.events(id, limit).await
    }

    /// One periodic bounded health pass; failed probes only notify at unhealthy/recovery transitions.
    pub async fn health_tick(&self) -> Result<Vec<Id>> {
        let mut recovered = Vec::new();
        for converter in self.repo.list().await?.into_iter().filter(|c| c.enabled) {
            let token = match self.cipher.decrypt(&converter.token_enc) {
                Ok(token) => token,
                Err(_) => {
                    self.record_health(converter.id, false, "token_decryption_failed", 0)
                        .await?;
                    continue;
                }
            };
            let started = Instant::now();
            let result = self.http.health(&converter.base_url, &token).await;
            let (ok, code) = match result {
                Ok(()) => (true, ""),
                Err(error) => (false, safe_error_code(&error)),
            };
            if self
                .record_health(converter.id, ok, code, elapsed_ms(started))
                .await?
            {
                recovered.push(converter.id);
            }
        }
        Ok(recovered)
    }

    async fn record_health(&self, id: Id, ok: bool, code: &str, duration_ms: i64) -> Result<bool> {
        let at = self.clock.now();
        let previous = self
            .repo
            .get(id)
            .await?
            .map(|c| c.health)
            .unwrap_or_default();
        let (saved, transitioned) = self
            .repo
            .health_result(id, ok, at, if ok { "" } else { code })
            .await?;
        self.repo
            .record_event(&NewConverterEvent {
                converter_id: id,
                event_type: "health_check".into(),
                status: if ok {
                    "healthy".into()
                } else {
                    saved.health.clone()
                },
                error_code: code.into(),
                duration_ms,
                order_no: String::new(),
                created_at: at,
            })
            .await?;
        if transitioned {
            self.notify_health(&saved, &previous).await;
        }
        Ok(ok && transitioned && saved.health == "healthy")
    }

    async fn notify_health(&self, converter: &Converter, previous: &str) {
        let recovered = converter.health == "healthy";
        let details = if recovered {
            format!("卡密转换器「{}」已恢复正常。", converter.name)
        } else {
            format!(
                "卡密转换器「{}」连续探测失败，健康状态为 {}。最近错误代码：{}。",
                converter.name, converter.health, converter.last_error
            )
        };
        let mut data = Map::new();
        data.insert("alert_type".into(), "card_converter_health".into());
        data.insert(
            "alert_level".into(),
            if recovered { "recovery" } else { "critical" }.into(),
        );
        data.insert("alert_value".into(), converter.health.clone().into());
        data.insert("alert_threshold".into(), "3 consecutive failures".into());
        data.insert("message".into(), details.into());
        data.insert(
            "affected_items_summary".into(),
            converter.name.clone().into(),
        );
        let _ = self
            .notifier
            .notify(NotifyEvent {
                event_type: events::EXCEPTION_ALERT.into(),
                biz_type: "card_converter".into(),
                biz_id: converter.id,
                locale: "zh-CN".into(),
                force: previous != converter.health,
                data,
            })
            .await;
    }

    pub async fn exchange(
        &self,
        converter_id: Id,
        request: &ExchangeRequest,
    ) -> Result<ExchangeResponse> {
        let converter = self.repo.get(converter_id).await?.ok_or_else(not_found)?;
        if !converter.enabled || converter.health == "unhealthy" {
            return Err(Error::internal_msg("card converter unavailable"));
        }
        let token = self
            .cipher
            .decrypt(&converter.token_enc)
            .map_err(Error::internal)?;
        let started = Instant::now();
        match self
            .http
            .exchange(&converter.base_url, &token, request)
            .await
        {
            Ok(response) => {
                self.repo
                    .record_event(&NewConverterEvent {
                        converter_id,
                        event_type: "exchange".into(),
                        status: "success".into(),
                        error_code: String::new(),
                        duration_ms: elapsed_ms(started),
                        order_no: request.order_no.clone(),
                        created_at: self.clock.now(),
                    })
                    .await?;
                Ok(response)
            }
            Err(error) => {
                let code = safe_error_code(&error);
                self.repo
                    .record_event(&NewConverterEvent {
                        converter_id,
                        event_type: "exchange".into(),
                        status: "failed".into(),
                        error_code: code.into(),
                        duration_ms: elapsed_ms(started),
                        order_no: request.order_no.clone(),
                        created_at: self.clock.now(),
                    })
                    .await?;
                if matches!(
                    code,
                    "authentication_failed"
                        | "timeout"
                        | "redirect_refused"
                        | "service_unavailable"
                ) {
                    self.record_health(converter_id, false, code, elapsed_ms(started))
                        .await?;
                }
                Err(error)
            }
        }
    }

    /// Converts a card delivery for an order when its product/SKU has a binding.
    /// The caller invokes this before persisting the fulfillment, outside any DB transaction.
    pub async fn convert_delivery(
        &self,
        order: &Order,
        delivery: &UpstreamDelivery,
        site: Value,
    ) -> Result<UpstreamDelivery> {
        if delivery.kind.trim() != "auto" {
            return Ok(delivery.clone());
        }
        if order.items.len() != 1 {
            return Err(Error::bad_request(
                "error.card_converter_order_shape_invalid",
            ));
        }
        let item = &order.items[0];
        let Some(binding) = self
            .repo
            .get_binding(item.product_id, item.sku_id)
            .await?
            .or(self.repo.get_binding(item.product_id, 0).await?)
        else {
            return Ok(delivery.clone());
        };
        let cards = delivery
            .payload
            .lines()
            .map(str::trim)
            .filter(|card| !card.is_empty())
            .collect::<Vec<_>>();
        if cards.len() != usize::try_from(item.quantity).unwrap_or(0) || cards.is_empty() {
            return Err(Error::bad_request(
                "error.card_converter_card_count_mismatch",
            ));
        }
        let source = self.repo.source_for_order(order.id).await?;
        let local_cards = self.repo.local_card_notes(order.id).await?;
        let local_cards_match = local_cards.len() == cards.len()
            && local_cards
                .iter()
                .zip(&cards)
                .all(|((secret, _, _), card)| secret == card);
        if !local_cards.is_empty() && !local_cards_match {
            return Err(Error::internal_msg(
                "local card provenance does not match delivery",
            ));
        }
        if local_cards_match {
            let preconverted = local_cards.iter().filter(|(_, _, ready)| *ready).count();
            if preconverted == cards.len() {
                return Ok(delivery.clone());
            }
            if preconverted > 0 {
                return Err(Error::internal_msg(
                    "mixed preconverted and raw cards require separate orders",
                ));
            }
        }
        let card_notes = if local_cards_match {
            local_cards
                .into_iter()
                .map(|(_, note, _)| note)
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        let shared_note = card_notes
            .first()
            .filter(|note| !note.is_empty() && card_notes.iter().all(|other| other == *note));
        let context = serde_json::json!({
            "order": {"id":order.id,"order_no":order.order_no,"quantity":item.quantity,"total_amount":order.total_amount.to_string()},
            "site": site,
            "product": {"id":item.product_id,"title":item.title},
            "sku": {"id":item.sku_id,"specifications":item.sku_snapshot},
            "source": source.map(|source| serde_json::json!({"id":source.id,"name":source.name,"protocol":source.protocol})),
            "card": {"note": shared_note, "notes": card_notes},
        });
        let extra = render_extra(&binding.extra_template, &context, &cards)?;
        let request = ExchangeRequest {
            order_no: order.order_no.clone(),
            items: cards
                .iter()
                .enumerate()
                .map(|(index, card)| ExchangeItem {
                    index,
                    upstream_card: (*card).to_owned(),
                })
                .collect(),
            extra,
        };
        let response = self.exchange(binding.converter_id, &request).await?;
        let mut outputs = vec![None; cards.len()];
        for output in response.items {
            let slot = outputs
                .get_mut(output.index)
                .ok_or_else(|| Error::internal_msg("card converter returned an invalid index"))?;
            *slot = Some(output.card);
        }
        let payload = outputs
            .into_iter()
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| Error::internal_msg("card converter returned incomplete output"))?
            .join("\n");
        let mut converted = delivery.clone();
        converted.payload = payload;
        Ok(converted)
    }
}

fn render_template(value: &Value, context: &Value) -> Result<Value> {
    match value {
        Value::String(raw) => {
            validate_placeholder_text(raw)?;
            let mut rendered = String::new();
            let mut rest = raw.as_str();
            while let Some(start) = rest.find("{{") {
                rendered.push_str(&rest[..start]);
                let after = &rest[start + 2..];
                let Some(end) = after.find("}}") else {
                    return Err(invalid());
                };
                let key = after[..end].trim();
                if !valid_variable(key) {
                    return Err(invalid());
                }
                let replacement = key
                    .split('.')
                    .try_fold(context, |node, part| node.get(part))
                    .and_then(|v| match v {
                        Value::String(s) => Some(s.clone()),
                        Value::Null => None,
                        other => Some(other.to_string()),
                    })
                    .ok_or_else(invalid)?;
                rest = &after[end + 2..];
                rendered.push_str(&replacement);
            }
            rendered.push_str(rest);
            Ok(Value::String(rendered))
        }
        Value::Array(items) => items
            .iter()
            .map(|item| render_template(item, context))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array),
        Value::Object(map) => map
            .iter()
            .map(|(key, item)| Ok((key.clone(), render_template(item, context)?)))
            .collect::<Result<serde_json::Map<_, _>>>()
            .map(Value::Object),
        _ => Ok(value.clone()),
    }
}

/// A rule is a JSON decision table. The first matching case wins.
const MAX_EXTRA_RULE_CASES: usize = 20;
const MAX_EXTRA_COMPARISON_BYTES: usize = 512;

fn validate_template(value: &Value) -> Result<()> {
    match value {
        Value::Object(map) if map.contains_key("$zs_rule") => {
            if map.len() != 1 {
                return Err(invalid());
            }
            let rule = map["$zs_rule"].as_object().ok_or_else(invalid)?;
            if rule.len() != 2 || !rule.contains_key("cases") || !rule.contains_key("else") {
                return Err(invalid());
            }
            let cases = rule["cases"].as_array().ok_or_else(invalid)?;
            if cases.is_empty() || cases.len() > MAX_EXTRA_RULE_CASES {
                return Err(invalid());
            }
            for case in cases {
                let case = case.as_object().ok_or_else(invalid)?;
                if case.len() != 2 || !case.contains_key("when") || !case.contains_key("then") {
                    return Err(invalid());
                }
                let condition = case["when"].as_object().ok_or_else(invalid)?;
                if condition.len() != 3 {
                    return Err(invalid());
                }
                let variable = condition
                    .get("variable")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid)?;
                let operator = condition
                    .get("operator")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid)?;
                let operand = condition
                    .get("value")
                    .and_then(Value::as_str)
                    .ok_or_else(invalid)?;
                if !(valid_variable(variable) || variable == "card" || variable == "card.notes")
                    || operand.len() > MAX_EXTRA_COMPARISON_BYTES
                {
                    return Err(invalid());
                }
                if !matches!(
                    operator,
                    "eq" | "ne" | "gt" | "gte" | "lt" | "lte" | "regex" | "regex_not"
                ) {
                    return Err(invalid());
                }
                if matches!(operator, "regex" | "regex_not") && Regex::new(operand).is_err() {
                    return Err(invalid());
                }
                validate_result(&case["then"])?;
            }
            validate_result(&rule["else"])?;
            Ok(())
        }
        Value::Object(map) => {
            for item in map.values() {
                validate_template(item)?;
            }
            Ok(())
        }
        Value::Array(items) => {
            for item in items {
                validate_template(item)?;
            }
            Ok(())
        }
        Value::String(raw) => validate_placeholder_text(raw),
        _ => Ok(()),
    }
}

const EXTRA_VARIABLES: &[&str] = &[
    "order.order_no",
    "order.quantity",
    "order.total_amount",
    "site.domain",
    "site.name",
    "product.id",
    "product.title",
    "sku.id",
    "sku.specifications",
    "source.id",
    "source.name",
    "source.protocol",
    "card.note",
];

fn valid_variable(path: &str) -> bool {
    EXTRA_VARIABLES.contains(&path)
}

fn validate_placeholder_text(raw: &str) -> Result<()> {
    let mut rest = raw;
    while let Some(start) = rest.find("{{") {
        if rest[..start].contains("}}") {
            return Err(invalid());
        }
        let after = &rest[start + 2..];
        let end = after.find("}}").ok_or_else(invalid)?;
        if !valid_variable(after[..end].trim()) {
            return Err(invalid());
        }
        rest = &after[end + 2..];
    }
    if rest.contains("}}") {
        return Err(invalid());
    }
    Ok(())
}

fn binding_issues(template: &Value, deps: &BindingDependencies) -> Vec<String> {
    let mut issues = Vec::new();
    if !deps.product_exists {
        issues.push("商品已删除，绑定不再生效".to_owned());
    }
    if !deps.sku_available {
        issues.push("绑定的 SKU 已删除或停用".to_owned());
    }
    if !deps.converter_enabled {
        issues.push("转换器已删除或停用".to_owned());
    }
    if deps.product_changed && template_uses_prefix(template, "product.") {
        issues.push("商品配置在绑定保存后已变更，请核对商品变量的发送值".to_owned());
    }
    if deps.sku_changed && template_uses_prefix(template, "sku.") {
        issues.push("SKU 配置在绑定保存后已变更，请核对 SKU 变量的发送值".to_owned());
    }
    if deps.source_changed
        && (template_uses_prefix(template, "source.name")
            || template_uses_prefix(template, "source.protocol"))
    {
        issues.push("上游名称或协议在绑定保存后可能已变更，请核对来源规则与变量".to_owned());
    }
    if deps.site_changed && template_uses_prefix(template, "site.") {
        issues.push("站点名称或域名在绑定保存后已变更，请核对站点变量的发送值".to_owned());
    }
    scan_source_dependencies(template, &deps.sources, &mut issues);
    issues.sort();
    issues.dedup();
    issues
}

fn template_uses_prefix(value: &Value, prefix: &str) -> bool {
    match value {
        Value::String(text) => text.contains(&format!("{{{{{prefix}")),
        Value::Object(map) => {
            map.get("variable")
                .and_then(Value::as_str)
                .is_some_and(|path| path.starts_with(prefix))
                || map
                    .values()
                    .any(|nested| template_uses_prefix(nested, prefix))
        }
        Value::Array(items) => items
            .iter()
            .any(|nested| template_uses_prefix(nested, prefix)),
        _ => false,
    }
}

fn scan_source_dependencies(
    value: &Value,
    sources: &[zs_domain::integration::card_converter::ConverterSource],
    issues: &mut Vec<String>,
) {
    match value {
        Value::String(text) if text.contains("{{source.") && sources.is_empty() => {
            issues.push("配置引用上游变量，但商品当前没有可用上游".to_owned());
        }
        Value::Object(map) if map.contains_key("$zs_rule") => {
            if let Some(cases) = map["$zs_rule"]["cases"].as_array() {
                for case in cases {
                    let when = &case["when"];
                    if when["operator"] == "eq" {
                        let expected = when["value"].as_str().unwrap_or_default();
                        let matched = match when["variable"].as_str().unwrap_or_default() {
                            "source.id" => sources
                                .iter()
                                .any(|source| source.id.to_string() == expected),
                            "source.name" => sources.iter().any(|source| source.name == expected),
                            "source.protocol" => {
                                sources.iter().any(|source| source.protocol == expected)
                            }
                            _ => true,
                        };
                        if !matched {
                            issues.push(format!(
                                "上游条件 {} = {} 已无可用匹配",
                                when["variable"].as_str().unwrap_or_default(),
                                expected
                            ));
                        }
                    }
                    scan_source_dependencies(&case["then"], sources, issues);
                }
            }
            scan_source_dependencies(&map["$zs_rule"]["else"], sources, issues);
        }
        Value::Object(map) => {
            if map
                .get("variable")
                .and_then(Value::as_str)
                .is_some_and(|path| path.starts_with("source."))
                && sources.is_empty()
            {
                issues.push("配置引用上游变量，但商品当前没有可用上游".to_owned());
            }
            for nested in map.values() {
                scan_source_dependencies(nested, sources, issues);
            }
        }
        Value::Array(items) => {
            for nested in items {
                scan_source_dependencies(nested, sources, issues);
            }
        }
        _ => {}
    }
}

fn validate_result(value: &Value) -> Result<()> {
    let map = value.as_object().ok_or_else(invalid)?;
    if map.len() != 1 {
        return Err(invalid());
    }
    if let Some(path) = map.get("variable") {
        if !path.as_str().is_some_and(valid_variable) {
            return Err(invalid());
        }
    } else if let Some(literal) = map.get("literal") {
        validate_template(literal)?;
    } else {
        return Err(invalid());
    }
    Ok(())
}

fn lookup<'a>(context: &'a Value, path: &str) -> Option<&'a Value> {
    path.split('.')
        .try_fold(context, |node, part| node.get(part))
}

fn condition_matches(condition: &Value, context: &Value, cards: &[&str]) -> Result<bool> {
    let path = condition["variable"].as_str().ok_or_else(invalid)?;
    let op = condition["operator"].as_str().ok_or_else(invalid)?;
    let expected = condition["value"].as_str().ok_or_else(invalid)?;
    let compare = |actual: &str| -> Result<bool> {
        Ok(match op {
            "eq" => actual == expected,
            "ne" => actual != expected,
            "gt" | "gte" | "lt" | "lte" => {
                let (Ok(a), Ok(b)) = (Decimal::from_str(actual), Decimal::from_str(expected))
                else {
                    return Ok(false);
                };
                match op {
                    "gt" => a > b,
                    "gte" => a >= b,
                    "lt" => a < b,
                    _ => a <= b,
                }
            }
            "regex" | "regex_not" => {
                let matched = Regex::new(expected)
                    .map_err(|_| invalid())?
                    .is_match(actual);
                if op == "regex" { matched } else { !matched }
            }
            _ => return Err(invalid()),
        })
    };
    if path == "card" {
        return cards
            .iter()
            .map(|card| compare(card))
            .collect::<Result<Vec<_>>>()
            .map(|matches| matches.into_iter().all(|matched| matched));
    }
    if path == "card.notes" {
        let notes = lookup(context, path)
            .and_then(Value::as_array)
            .ok_or_else(invalid)?;
        if notes.is_empty()
            || notes
                .iter()
                .any(|note| note.as_str().is_none_or(str::is_empty))
        {
            return Ok(false);
        }
        return notes
            .iter()
            .map(|note| compare(note.as_str().unwrap_or_default()))
            .collect::<Result<Vec<_>>>()
            .map(|matches| matches.into_iter().all(|matched| matched));
    }
    let Some(actual) = lookup(context, path).filter(|value| !value.is_null()) else {
        return Ok(false);
    };
    compare(
        actual
            .as_str()
            .map_or_else(|| actual.to_string(), str::to_owned)
            .as_str(),
    )
}

fn render_result(value: &Value, context: &Value, cards: &[&str]) -> Result<Value> {
    if let Some(path) = value.get("variable") {
        return lookup(context, path.as_str().ok_or_else(invalid)?)
            .filter(|value| !value.is_null())
            .cloned()
            .ok_or_else(invalid);
    }
    render_extra(value.get("literal").ok_or_else(invalid)?, context, cards)
}

fn render_extra(value: &Value, context: &Value, cards: &[&str]) -> Result<Value> {
    if let Value::Object(map) = value {
        if let Some(rule) = map.get("$zs_rule") {
            validate_template(value)?;
            for case in rule["cases"].as_array().ok_or_else(invalid)? {
                if condition_matches(&case["when"], context, cards)? {
                    return render_result(&case["then"], context, cards);
                }
            }
            return render_result(&rule["else"], context, cards);
        }
        return map
            .iter()
            .map(|(key, item)| Ok((key.clone(), render_extra(item, context, cards)?)))
            .collect::<Result<Map<_, _>>>()
            .map(Value::Object);
    }
    if let Value::Array(items) = value {
        return items
            .iter()
            .map(|item| render_extra(item, context, cards))
            .collect::<Result<Vec<_>>>()
            .map(Value::Array);
    }
    render_template(value, context)
}

fn mask(mut converter: Converter) -> Converter {
    converter.token_configured = !converter.token_enc.is_empty();
    converter.token_enc.clear();
    converter
}

fn normalize_url(raw: &str) -> Result<String> {
    let parsed = url::Url::parse(raw.trim()).map_err(|_| invalid())?;
    if parsed.scheme() != "https"
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(invalid());
    }
    Ok(raw.trim().trim_end_matches('/').to_owned())
}

fn safe_error_code(error: &Error) -> &'static str {
    let detail = error.to_string().to_ascii_lowercase();
    if detail.contains("401") || detail.contains("403") {
        "authentication_failed"
    } else if detail.contains("409") {
        "idempotency_conflict"
    } else if detail.contains("422") {
        "type_mismatch"
    } else if detail.contains("429") {
        "rate_limited"
    } else if detail.contains("400") {
        "invalid_request"
    } else if detail.contains("timeout") {
        "timeout"
    } else if detail.contains("redirect") {
        "redirect_refused"
    } else if detail.contains("too large") {
        "response_too_large"
    } else {
        "service_unavailable"
    }
}

fn elapsed_ms(started: Instant) -> i64 {
    i64::try_from(started.elapsed().as_millis()).unwrap_or(i64::MAX)
}

fn invalid() -> Error {
    Error::bad_request("error.card_converter_invalid")
}
fn not_found() -> Error {
    Error::not_found("error.card_converter_not_found")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn template_renders_allowed_paths_and_preserves_constants() {
        let context = serde_json::json!({
            "site": {"domain": "shop.example"},
            "product": {"title": "新激活"}
        });
        let template = serde_json::json!({
            "site": "{{site.domain}}",
            "product": "{{ product.title }}",
            "enabled": true
        });
        assert_eq!(
            render_template(&template, &context).ok(),
            Some(serde_json::json!({"site":"shop.example","product":"新激活","enabled":true}))
        );
    }

    #[test]
    fn template_rejects_unknown_and_malformed_paths() {
        let context = serde_json::json!({"site":{"domain":"shop.example"}});
        assert!(render_template(&serde_json::json!("{{site.secret}}"), &context).is_err());
        assert!(render_template(&serde_json::json!("{{site.domain"), &context).is_err());
        assert!(validate_template(&serde_json::json!({"a":"{{buyer.email}}"})).is_err());
        assert!(validate_template(&serde_json::json!({"a":"{{site.domain}"})).is_err());
        assert!(validate_template(&serde_json::json!({"a":"site.domain}}"})).is_err());
        assert!(validate_template(&serde_json::json!({"a":"{{source.unknown}}"})).is_err());
    }

    #[test]
    fn extra_contract_lists_the_same_variables_and_limits_as_admin() {
        let contract: Value = serde_json::from_str(include_str!(
            "../../../../../docs/protocol/card-converter-extra-contract.json"
        ))
        .expect("checked-in contract JSON is valid");
        assert_eq!(contract["variables"], serde_json::json!(EXTRA_VARIABLES));
        assert_eq!(
            contract["condition_only_variables"],
            serde_json::json!(["card", "card.notes"])
        );
        assert_eq!(
            contract["operators"],
            serde_json::json!(["eq", "ne", "gt", "gte", "lt", "lte", "regex", "regex_not"])
        );
        assert_eq!(contract["max_cases"], MAX_EXTRA_RULE_CASES);
        assert_eq!(contract["max_comparison_bytes"], MAX_EXTRA_COMPARISON_BYTES);
    }

    #[test]
    fn all_extra_variables_resolve_and_json_is_escaped() {
        let context = serde_json::json!({
            "order":{"order_no":"ZS-1","quantity":2,"total_amount":"12.30"},
            "site":{"domain":"a\"b.example","name":"店铺"},
            "product":{"id":12,"title":"激活"},
            "sku":{"id":34,"specifications":{"region":"US"}},
            "source":{"id":7,"name":"渠道1","protocol":"acg_faka"}
        });
        let template = serde_json::json!({
            "order_no":"{{order.order_no}}","quantity":"{{order.quantity}}",
            "amount":"{{order.total_amount}}","domain":"{{site.domain}}",
            "site_name":"{{site.name}}","product_id":"{{product.id}}",
            "product_title":"{{product.title}}","sku_id":"{{sku.id}}",
            "specs":"{{sku.specifications}}","source_id":"{{source.id}}",
            "source_name":"{{source.name}}","source_protocol":"{{source.protocol}}",
            "typed_quantity":{"$zs_rule":{"cases":[{"when":{"variable":"order.quantity","operator":"gte","value":"2"},"then":{"variable":"order.quantity"}}],"else":{"literal":0}}}
        });
        assert!(validate_template(&template).is_ok());
        let rendered =
            render_extra(&template, &context, &["P1-X"]).expect("known variables resolve");
        assert_eq!(rendered["quantity"], "2");
        assert_eq!(rendered["typed_quantity"], 2);
        assert_eq!(rendered["domain"], "a\"b.example");
        assert_eq!(rendered["specs"], "{\"region\":\"US\"}");
        assert_eq!(rendered["source_name"], "渠道1");
        assert_eq!(rendered["source_protocol"], "acg_faka");
        assert_eq!(
            serde_json::from_str::<Value>(&rendered.to_string()).ok(),
            Some(rendered)
        );
    }

    #[test]
    fn decision_table_uses_source_numeric_comparison_and_card_regex() {
        let context = serde_json::json!({"source":{"name":"A"},"order":{"quantity":2}});
        let template = serde_json::json!({
            "provider":{"$zs_rule":{"cases":[
                {"when":{"variable":"source.name","operator":"eq","value":"A"},"then":{"literal":"p1"}},
                {"when":{"variable":"card","operator":"regex","value":"^P2-"},"then":{"literal":"p2"}}
            ],"else":{"literal":"other"}}},
            "category":{"$zs_rule":{"cases":[
                {"when":{"variable":"order.quantity","operator":"gte","value":"2"},"then":{"literal":"go"}}
            ],"else":{"literal":"single"}}}
        });
        assert!(validate_template(&template).is_ok());
        assert_eq!(
            render_extra(&template, &context, &["P2-1", "P2-2"]).ok(),
            Some(serde_json::json!({"provider":"p1","category":"go"}))
        );
        let fallback = serde_json::json!({"provider":{"$zs_rule":{"cases":[{"when":{"variable":"card","operator":"regex","value":"^P2-"},"then":{"literal":"p2"}}],"else":{"literal":"other"}}}});
        assert_eq!(
            render_extra(&fallback, &context, &["P2-1", "bad"]).ok(),
            Some(serde_json::json!({"provider":"other"}))
        );
        assert_eq!(
            render_extra(&fallback, &context, &["P2-1", "P2-2"]).ok(),
            Some(serde_json::json!({"provider":"p2"}))
        );
    }

    #[test]
    fn imported_card_notes_match_only_when_every_card_has_that_source() {
        let rule = serde_json::json!({"provider":{"$zs_rule":{"cases":[
            {"when":{"variable":"card.notes","operator":"eq","value":"渠道 A"},"then":{"literal":"p1"}},
            {"when":{"variable":"card.notes","operator":"regex","value":"^渠道 B$"},"then":{"literal":"p2"}}
        ],"else":{"literal":"manual_review"}}}});
        let one_source = serde_json::json!({"card":{"note":"渠道 A","notes":["渠道 A","渠道 A"]}});
        assert_eq!(
            render_extra(&rule, &one_source, &["A-1", "A-2"]).ok(),
            Some(serde_json::json!({"provider":"p1"}))
        );
        assert_eq!(
            render_template(&serde_json::json!("{{card.note}}"), &one_source).ok(),
            Some(serde_json::json!("渠道 A"))
        );
        let mixed = serde_json::json!({"card":{"note":null,"notes":["渠道 A","渠道 B"]}});
        assert_eq!(
            render_extra(&rule, &mixed, &["A-1", "B-1"]).ok(),
            Some(serde_json::json!({"provider":"manual_review"}))
        );
        assert!(render_template(&serde_json::json!("{{card.note}}"), &mixed).is_err());
        let upstream = serde_json::json!({"card":{"note":null,"notes":[]}});
        assert_eq!(
            render_extra(&rule, &upstream, &["external"]).ok(),
            Some(serde_json::json!({"provider":"manual_review"}))
        );
        let missing = serde_json::json!({"card":{"note":null,"notes":[""]}});
        let not_a = serde_json::json!({"provider":{"$zs_rule":{"cases":[
            {"when":{"variable":"card.notes","operator":"ne","value":"渠道 A"},"then":{"literal":"p2"}}
        ],"else":{"literal":"manual_review"}}}});
        assert_eq!(
            render_extra(&not_a, &missing, &["legacy"]).ok(),
            Some(serde_json::json!({"provider":"manual_review"}))
        );
    }

    #[test]
    fn decision_table_rejects_invalid_regex_and_missing_result() {
        let invalid_regex = serde_json::json!({"$zs_rule":{"cases":[{"when":{"variable":"card","operator":"regex","value":"["},"then":{"literal":"x"}}],"else":{"literal":"y"}}});
        assert!(validate_template(&invalid_regex).is_err());
        let missing_else = serde_json::json!({"$zs_rule":{"cases":[]}});
        assert!(validate_template(&missing_else).is_err());
    }

    #[test]
    fn extra_rule_boundaries_preserve_json_types_and_use_default_for_missing_source() {
        let context = serde_json::json!({"source":null,"order":{"quantity":2,"total_amount":"12.30"},"site":{"domain":"a\"b.example"}});
        let template = serde_json::json!({
            "route":{"$zs_rule":{"cases":[{"when":{"variable":"source.id","operator":"eq","value":"7"},"then":{"literal":"p1"}}],"else":{"literal":"manual"}}},
            "quantity":{"$zs_rule":{"cases":[{"when":{"variable":"order.quantity","operator":"gte","value":"2"},"then":{"variable":"order.quantity"}}],"else":{"literal":0}}},
            "amount":{"$zs_rule":{"cases":[{"when":{"variable":"order.total_amount","operator":"lt","value":"12.31"},"then":{"literal":true}}],"else":{"literal":false}}},
            "note":"{{site.domain}}"
        });
        assert_eq!(
            render_extra(&template, &context, &["P1-X"]).ok(),
            Some(
                serde_json::json!({"route":"manual","quantity":2,"amount":true,"note":"a\"b.example"})
            )
        );

        let source = serde_json::json!({"source":{"id":7}});
        let route = serde_json::json!({"$zs_rule":{"cases":[{"when":{"variable":"source.id","operator":"eq","value":"7"},"then":{"literal":"p1"}}],"else":{"literal":"other"}}});
        assert_eq!(
            render_extra(&route, &source, &["P1-X"]).ok(),
            Some(serde_json::json!("p1"))
        );
    }

    #[test]
    fn extra_rule_rejects_unknown_paths_operators_and_oversized_cases() {
        let invalid_path = serde_json::json!({"$zs_rule":{"cases":[{"when":{"variable":"buyer.email","operator":"eq","value":"x"},"then":{"literal":"x"}}],"else":{"literal":"y"}}});
        assert!(validate_template(&invalid_path).is_err());
        let invalid_operator = serde_json::json!({"$zs_rule":{"cases":[{"when":{"variable":"source.id","operator":"execute","value":"x"},"then":{"literal":"x"}}],"else":{"literal":"y"}}});
        assert!(validate_template(&invalid_operator).is_err());
        let too_many = serde_json::json!({"$zs_rule":{"cases":vec![serde_json::json!({"when":{"variable":"source.id","operator":"eq","value":"7"},"then":{"literal":"x"}});21],"else":{"literal":"y"}}});
        assert!(validate_template(&too_many).is_err());
    }

    #[test]
    fn extra_condition_operators_handle_boundaries_and_multiple_cards() {
        let context = serde_json::json!({"order":{"quantity":2},"source":{"id":7,"name":"渠道1"}});
        let cases = [
            ("eq", "2", true),
            ("ne", "2", false),
            ("ne", "3", true),
            ("gt", "2", false),
            ("gte", "2", true),
            ("lt", "2", false),
            ("lte", "2", true),
            ("gt", "not-a-number", false),
        ];
        for (operator, value, expected) in cases {
            let condition =
                serde_json::json!({"variable":"order.quantity","operator":operator,"value":value});
            assert_eq!(
                condition_matches(&condition, &context, &["P1-A"]).ok(),
                Some(expected),
                "{operator} {value}"
            );
        }
        let regex = serde_json::json!({"variable":"card","operator":"regex","value":"^P1-"});
        let inverse = serde_json::json!({"variable":"card","operator":"regex_not","value":"^P1-"});
        assert_eq!(
            condition_matches(&regex, &context, &["P1-A", "P1-B"]).ok(),
            Some(true)
        );
        assert_eq!(
            condition_matches(&regex, &context, &["P1-A", "P2-B"]).ok(),
            Some(false)
        );
        assert_eq!(
            condition_matches(&inverse, &context, &["P2-A", "P2-B"]).ok(),
            Some(true)
        );
        assert_eq!(
            condition_matches(&inverse, &context, &["P1-A", "P2-B"]).ok(),
            Some(false)
        );
    }

    #[test]
    fn extra_rejects_unsupported_variables_and_utf8_oversized_comparisons() {
        let invalid_variable = serde_json::json!({"route":{"$zs_rule":{"cases":[{"when":{"variable":"source.other","operator":"eq","value":"x"},"then":{"literal":"p1"}}],"else":{"literal":"other"}}}});
        assert!(validate_template(&invalid_variable).is_err());
        let invalid_result = serde_json::json!({"route":{"$zs_rule":{"cases":[{"when":{"variable":"source.id","operator":"eq","value":"7"},"then":{"variable":"card"}}],"else":{"literal":"other"}}}});
        assert!(validate_template(&invalid_result).is_err());
        let oversized = serde_json::json!({"route":{"$zs_rule":{"cases":[{"when":{"variable":"source.name","operator":"eq","value":"中".repeat(171)},"then":{"literal":"p1"}}],"else":{"literal":"other"}}}});
        assert!(validate_template(&oversized).is_err());
        let boundary = serde_json::json!({"route":{"$zs_rule":{"cases":[{"when":{"variable":"source.name","operator":"eq","value":"a".repeat(512)},"then":{"literal":"p1"}}],"else":{"literal":"other"}}}});
        assert!(validate_template(&boundary).is_ok());
    }

    #[test]
    fn extra_binding_diagnostics_find_deleted_or_renamed_dependencies() {
        let template = serde_json::json!({
            "provider":{"$zs_rule":{"cases":[
                {"when":{"variable":"source.id","operator":"eq","value":"7"},"then":{"literal":"p1"}},
                {"when":{"variable":"source.name","operator":"eq","value":"旧名称"},"then":{"literal":"old"}}
            ],"else":{"literal":"other"}}},
            "source_name":"{{source.name}}"
        });
        let deps = BindingDependencies {
            product_exists: true,
            sku_available: true,
            converter_enabled: true,
            sources: vec![zs_domain::integration::card_converter::ConverterSource {
                id: 7,
                name: "渠道1".into(),
                protocol: "acg_faka".into(),
            }],
            product_changed: false,
            sku_changed: false,
            source_changed: false,
            site_changed: false,
        };
        assert_eq!(
            binding_issues(&template, &deps),
            vec!["上游条件 source.name = 旧名称 已无可用匹配"]
        );
        let missing = BindingDependencies {
            product_exists: false,
            sku_available: false,
            converter_enabled: false,
            sources: Vec::new(),
            product_changed: false,
            sku_changed: false,
            source_changed: false,
            site_changed: false,
        };
        let issues = binding_issues(&template, &missing);
        assert!(issues.iter().any(|issue| issue.contains("商品已删除")));
        assert!(issues.iter().any(|issue| issue.contains("SKU")));
        assert!(issues.iter().any(|issue| issue.contains("转换器")));
        assert!(issues.iter().any(|issue| issue.contains("source.id = 7")));
        assert!(issues.iter().any(|issue| issue.contains("没有可用上游")));
        let changed = BindingDependencies {
            product_changed: true,
            sku_changed: true,
            source_changed: true,
            site_changed: true,
            ..deps
        };
        let dynamic = serde_json::json!({"product":"{{product.title}}","sku":"{{sku.specifications}}","source":"{{source.name}}","site":"{{site.domain}}"});
        let issues = binding_issues(&dynamic, &changed);
        assert!(issues.iter().any(|issue| issue.contains("商品配置")));
        assert!(issues.iter().any(|issue| issue.contains("SKU 配置")));
        assert!(issues.iter().any(|issue| issue.contains("上游名称")));
        assert!(issues.iter().any(|issue| issue.contains("站点名称")));
    }
}
