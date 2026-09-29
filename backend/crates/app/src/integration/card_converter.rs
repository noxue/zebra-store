//! Management, health monitoring and protocol calls for optional card converters.

use std::sync::Arc;
use std::time::Instant;

use serde_json::{Map, Value};
use zs_domain::integration::card_converter::{
    Binding, Converter, ConverterEvent, ConverterHttp, ConverterInput, ConverterRepo, ExchangeItem,
    ExchangeRequest, ExchangeResponse, NewConverterEvent,
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
        let mut saved = self
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
        // A saved connection is immediately checked so its first visible status is useful.
        if input.enabled {
            let _ = self.refresh_types(saved.id).await;
            saved = self.repo.get(saved.id).await?.ok_or_else(not_found)?;
        }
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

    pub async fn refresh_types(
        &self,
        id: Id,
    ) -> Result<Vec<zs_domain::integration::card_converter::ConverterType>> {
        let mut converter = self.repo.get(id).await?.ok_or_else(not_found)?;
        let token = self
            .cipher
            .decrypt(&converter.token_enc)
            .map_err(Error::internal)?;
        let started = Instant::now();
        match self.http.types(&converter.base_url, &token).await {
            Ok(types) => {
                converter.types = types.clone();
                converter.health = "healthy".into();
                converter.consecutive_failures = 0;
                converter.last_checked_at = Some(self.clock.now());
                converter.last_error.clear();
                self.repo.save(&converter).await?;
                self.repo
                    .record_event(&NewConverterEvent {
                        converter_id: id,
                        event_type: "type_discovery".into(),
                        status: "success".into(),
                        error_code: String::new(),
                        duration_ms: elapsed_ms(started),
                        order_no: String::new(),
                        created_at: self.clock.now(),
                    })
                    .await?;
                Ok(types)
            }
            Err(error) => {
                let safe = safe_error_code(&error);
                self.repo
                    .record_event(&NewConverterEvent {
                        converter_id: id,
                        event_type: "type_discovery".into(),
                        status: "failed".into(),
                        error_code: safe.into(),
                        duration_ms: elapsed_ms(started),
                        order_no: String::new(),
                        created_at: self.clock.now(),
                    })
                    .await?;
                Err(error)
            }
        }
    }

    pub async fn test(&self, id: Id) -> Result<Value> {
        let types = self.refresh_types(id).await?;
        Ok(serde_json::json!({"ok":true,"type_count":types.len()}))
    }

    pub async fn bindings(&self, product_id: Option<Id>) -> Result<Vec<Binding>> {
        self.repo.bindings(product_id).await
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
        if !converter.enabled || !converter.types.iter().any(|t| t.id == binding.type_id) {
            return Err(Error::bad_request("error.card_converter_type_invalid"));
        }
        let allowed = [
            "order.order_no",
            "order.quantity",
            "order.currency",
            "site.domain",
            "site.name",
            "product.id",
            "product.slug",
            "product.title",
            "product.variables",
            "sku.id",
            "sku.code",
            "sku.specifications",
        ];
        binding.fields.sort();
        binding.fields.dedup();
        if binding
            .fields
            .iter()
            .any(|field| !allowed.contains(&field.as_str()))
        {
            return Err(invalid());
        }
        if !binding.extra_template.is_object() && !binding.extra_template.is_null() {
            return Err(invalid());
        }
        if !binding.extra_template.is_object() {
            binding.extra_template = Value::Object(Map::new());
        }
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
                        order_no: request.order["order_no"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
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
                        order_no: request.order["order_no"]
                            .as_str()
                            .unwrap_or_default()
                            .to_owned(),
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
        let context = serde_json::json!({
            "order": {"id":order.id,"order_no":order.order_no,"quantity":item.quantity,"total_amount":order.total_amount.to_string()},
            "site": site,
            "product": {"id":item.product_id,"title":item.title},
            "sku": {"id":item.sku_id,"specifications":item.sku_snapshot},
        });
        let extra = render_template(&binding.extra_template, &context)?;
        let request = ExchangeRequest {
            protocol_version: "1".into(),
            idempotency_key: format!("zebra-order-{}-item-{}", order.id, item.id),
            type_id: binding.type_id.clone(),
            order: context["order"].clone(),
            site: context["site"].clone(),
            product: context["product"].clone(),
            sku: context["sku"].clone(),
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
            let mut rendered = String::new();
            let mut rest = raw.as_str();
            while let Some(start) = rest.find("{{") {
                rendered.push_str(&rest[..start]);
                let after = &rest[start + 2..];
                let Some(end) = after.find("}}") else {
                    return Err(invalid());
                };
                let key = after[..end].trim();
                if key.is_empty() || key.split('.').any(|part| part.is_empty()) {
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
    fn template_renders_nested_paths_and_preserves_constants() {
        let context = serde_json::json!({
            "site": {"domain": "shop.example"},
            "product": {"variables": {"region": "eu"}}
        });
        let template = serde_json::json!({
            "site": "{{site.domain}}",
            "region": "{{ product.variables.region }}",
            "enabled": true
        });
        assert_eq!(
            render_template(&template, &context).ok(),
            Some(serde_json::json!({"site":"shop.example","region":"eu","enabled":true}))
        );
    }

    #[test]
    fn template_rejects_unknown_and_malformed_paths() {
        let context = serde_json::json!({"site":{"domain":"shop.example"}});
        assert!(render_template(&serde_json::json!("{{site.secret}}"), &context).is_err());
        assert!(render_template(&serde_json::json!("{{site.domain"), &context).is_err());
    }
}
