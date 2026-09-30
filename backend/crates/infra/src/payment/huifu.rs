//! Huifu hosted Alipay/WeChat payments, signed notifications and original-route refunds.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::{FixedOffset, Offset, Utc};
use huifu_pay::{
    Client, Config as SdkConfig, Error as SdkError, HttpRequest as SdkRequest,
    HttpResponse as SdkResponse, PaymentQueryRequest, PreorderRequest, RefundQueryRequest,
    RefundRequest, TradeBillQueryRequest, Transport,
};
use serde::Deserialize;
use serde_json::Value;
use zs_domain::payment::channel::ChannelConfig;
use zs_domain::payment::form::{FormMap, form_raw};
use zs_domain::payment::gateway::{
    GatewayCallbackResult, GatewayCapabilities, GatewayCreateInput, GatewayCreateResult,
    GatewayError, GatewayQueryResult, GatewayRefundInput, GatewayRefundResult,
    GatewayTradeBillDownload, GatewayTradeBillFile, GatewayTradeBillQuery, GatewayTradeBillTask,
    PaymentGateway,
};
use zs_domain::payment::returns::append_query_params;
use zs_domain::payment::types::{InteractionMode, PaymentStatus, channel_type};

use super::common::{GatewayEnv, callback_amount, parse_config};
use super::http::{HttpRequest, HttpTransport};

const DEFAULT_API: &str = huifu_pay::DEFAULT_BASE_URL;
const LOCAL_SANDBOX_SKILL_SOURCE: &str = "hfps/1.3.1;sandbox/1.0.0";

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct Config {
    pub api_base_url: String,
    pub sys_id: String,
    pub product_id: String,
    pub huifu_id: String,
    pub merchant_private_key: String,
    pub huifu_public_key: String,
    pub project_id: String,
    pub project_title: String,
    pub notify_url: String,
    pub return_url: String,
}

impl Config {
    fn parse(raw: &ChannelConfig) -> Result<Self, GatewayError> {
        let mut c: Self = parse_config(raw, "huifu")?;
        if c.api_base_url.trim().is_empty() {
            c.api_base_url = DEFAULT_API.into();
        }
        if c.project_title.trim().is_empty() {
            c.project_title = "Zebra Store".into();
        }
        c.validate()?;
        Ok(c)
    }

    fn validate(&self) -> Result<(), GatewayError> {
        for (name, value) in [
            ("sys_id", &self.sys_id),
            ("product_id", &self.product_id),
            ("huifu_id", &self.huifu_id),
            ("merchant_private_key", &self.merchant_private_key),
            ("huifu_public_key", &self.huifu_public_key),
            ("project_id", &self.project_id),
            ("notify_url", &self.notify_url),
            ("return_url", &self.return_url),
        ] {
            if value.trim().is_empty() {
                return Err(GatewayError::config(format!(
                    "huifu config invalid: {name} is required"
                )));
            }
        }
        self.sdk().map(|_| ())
    }

    fn sdk(&self) -> Result<SdkConfig, GatewayError> {
        // Huifu's official local sandbox preview is pinned to skill 1.3.1 and requires its
        // sandbox suffix. Production and official online testing always use the current fixed
        // integration source from huifu-pay.
        let skill_source = if self.api_base_url.starts_with("http://127.0.0.1:")
            || self.api_base_url.starts_with("http://localhost:")
        {
            LOCAL_SANDBOX_SKILL_SOURCE
        } else {
            huifu_pay::DEFAULT_SKILL_SOURCE
        };
        let c = SdkConfig {
            base_url: self.api_base_url.trim().to_owned(),
            sys_id: self.sys_id.trim().to_owned(),
            product_id: self.product_id.trim().to_owned(),
            huifu_id: self.huifu_id.trim().to_owned(),
            merchant_private_key: self.merchant_private_key.trim().to_owned(),
            huifu_public_key: self.huifu_public_key.trim().to_owned(),
            skill_source: skill_source.to_owned(),
        };
        c.validate().map_err(map_error)?;
        Ok(c)
    }
}

#[derive(Debug)]
struct InfraTransport(Arc<dyn HttpTransport>);

#[async_trait]
impl Transport for InfraTransport {
    async fn post(&self, request: SdkRequest) -> Result<SdkResponse, SdkError> {
        let mut outbound = HttpRequest::new("POST", request.url).body(request.body);
        outbound.headers = request.headers;
        let response = self.0.send(outbound).await.map_err(SdkError::Transport)?;
        Ok(SdkResponse {
            status: response.status,
            body: response.body,
        })
    }

    async fn get(&self, request: SdkRequest) -> Result<SdkResponse, SdkError> {
        let mut outbound = HttpRequest::new("GET", request.url);
        outbound.headers = request.headers;
        let response = self.0.send(outbound).await.map_err(SdkError::Transport)?;
        Ok(SdkResponse {
            status: response.status,
            body: response.body,
        })
    }
}

#[derive(Debug, Clone)]
pub struct HuifuGateway {
    env: GatewayEnv,
}

impl HuifuGateway {
    pub fn new(env: GatewayEnv) -> Self {
        Self { env }
    }

    fn client(&self, raw: &ChannelConfig) -> Result<(Config, Client), GatewayError> {
        let config = Config::parse(raw)?;
        let client = Client::with_transport(
            config.sdk()?,
            Arc::new(InfraTransport(self.env.http.clone())),
        )
        .map_err(map_error)?;
        Ok((config, client))
    }

    fn date(&self) -> String {
        let offset = FixedOffset::east_opt(8 * 3600).unwrap_or_else(|| Utc.fix());
        self.env
            .clock
            .now()
            .with_timezone(&offset)
            .format("%Y%m%d")
            .to_string()
    }

    fn query_no(&self, prefix: &str) -> String {
        format!("{prefix}{}", self.env.random_hex(12))
    }
}

fn locator(date: &str, sequence: &str) -> String {
    format!("{}:{}", date.trim(), sequence.trim())
}

fn split_locator(value: &str) -> Result<(&str, &str), GatewayError> {
    let (date, sequence) = value
        .trim()
        .split_once(':')
        .ok_or_else(|| GatewayError::response("huifu provider reference is invalid"))?;
    if date.len() != 8 || sequence.is_empty() {
        return Err(GatewayError::response(
            "huifu provider reference is invalid",
        ));
    }
    Ok((date, sequence))
}

fn map_status(raw: &str) -> Option<PaymentStatus> {
    match raw.trim().to_ascii_uppercase().as_str() {
        "P" | "I" => Some(PaymentStatus::Pending),
        "S" => Some(PaymentStatus::Success),
        "F" => Some(PaymentStatus::Failed),
        _ => None,
    }
}

fn map_error(error: SdkError) -> GatewayError {
    match error {
        SdkError::Config(e) | SdkError::Request(e) => GatewayError::config(e),
        SdkError::Signature => GatewayError::signature("huifu RSA-SHA256 verification failed"),
        SdkError::Response(e) => GatewayError::response(e),
        SdkError::Transport(e) => GatewayError::request(e),
        SdkError::Http(status) => GatewayError::request(format!("huifu HTTP {status}")),
    }
}

fn require_accepted(response: &huifu_pay::ApiResponse) -> Result<(), GatewayError> {
    if response.accepted() {
        return Ok(());
    }
    let code = response.string("resp_code");
    let description = response.string("resp_desc");
    if code == "00000009" && description.contains("统一收银台") {
        return Err(GatewayError::ProviderPermissionMissing(format!(
            "huifu {code}: {description}"
        )));
    }
    Err(GatewayError::response(format!(
        "huifu rejected request: {code} {description}"
    )))
}

fn huifu_request_type(user_agent: &str) -> &'static str {
    let ua = user_agent.to_ascii_lowercase();
    if [
        "android",
        "iphone",
        "ipad",
        "ipod",
        "mobile",
        "micromessenger",
    ]
    .iter()
    .any(|marker| ua.contains(marker))
    {
        "M"
    } else {
        "P"
    }
}

#[async_trait]
impl PaymentGateway for HuifuGateway {
    fn key(&self) -> &'static str {
        "huifu:"
    }

    fn capabilities(&self) -> GatewayCapabilities {
        GatewayCapabilities {
            query: true,
            callback: true,
            trade_bills: true,
            ..GatewayCapabilities::default()
        }
    }

    fn validate_config(&self, config: &ChannelConfig, channel: &str) -> Result<(), GatewayError> {
        if !matches!(channel.trim(), channel_type::ALIPAY | channel_type::WECHAT) {
            return Err(GatewayError::UnsupportedChannel(channel.to_owned()));
        }
        Config::parse(config).map(|_| ())
    }

    async fn create_payment(
        &self,
        raw: &ChannelConfig,
        input: &GatewayCreateInput,
    ) -> Result<GatewayCreateResult, GatewayError> {
        let direct = match input.interaction_mode {
            Some(InteractionMode::Qr) => true,
            Some(InteractionMode::Redirect) => false,
            _ => {
                return Err(GatewayError::UnsupportedChannel(
                    "huifu supports qr and redirect interaction modes".into(),
                ));
            }
        };
        let trans_type = match input.channel_type.trim() {
            channel_type::ALIPAY => "A_NATIVE",
            channel_type::WECHAT => "T_JSAPI",
            other => return Err(GatewayError::UnsupportedChannel(other.to_owned())),
        };
        let (config, client) = self.client(raw)?;
        let date = self.date();
        let notify_url = if input.notify_url.trim().is_empty() {
            config.notify_url.clone()
        } else {
            input.notify_url.trim().to_owned()
        };
        let return_url = if input.return_url.trim().is_empty() {
            config.return_url.clone()
        } else {
            append_query_params(&input.return_url, &input.return_url_query)
        };
        let response = client
            .preorder(&PreorderRequest {
                req_date: date.clone(),
                req_seq_id: input.order_no.clone(),
                trans_amt: input.amount.to_string(),
                goods_desc: input.subject.clone(),
                notify_url,
                callback_url: return_url,
                project_id: config.project_id,
                project_title: config.project_title,
                request_type: huifu_request_type(&input.user_agent).into(),
                trans_type: trans_type.into(),
                time_expire: String::new(),
            })
            .await
            .map_err(map_error)?;
        require_accepted(&response)?;
        let jump_url = response.string("jump_url");
        if jump_url.trim().is_empty() {
            return Err(GatewayError::response(
                "huifu response invalid: jump_url is empty",
            ));
        }
        let mut payload = response.data.clone();
        payload.insert("req_date".into(), Value::String(date.clone()));
        payload.insert("req_seq_id".into(), Value::String(input.order_no.clone()));
        Ok(GatewayCreateResult {
            provider_ref: locator(&date, &input.order_no),
            redirect_url: jump_url.clone(),
            // In direct mode desktop clients encode the signed hosted URL as a
            // QR code; mobile clients open the same URL so Huifu can invoke the
            // selected app. Redirect mode leaves the QR value empty.
            qr_code_url: if direct { jump_url } else { String::new() },
            payload,
            display_channel_type: input.channel_type.clone(),
            ..GatewayCreateResult::default()
        })
    }

    async fn query_payment(
        &self,
        raw: &ChannelConfig,
        provider_ref: &str,
    ) -> Result<GatewayQueryResult, GatewayError> {
        let (_, client) = self.client(raw)?;
        let (org_date, org_sequence) = split_locator(provider_ref)?;
        let response = client
            .query_payment(&PaymentQueryRequest {
                req_date: self.date(),
                req_seq_id: self.query_no("ZSQ"),
                org_req_date: org_date.into(),
                org_req_seq_id: org_sequence.into(),
            })
            .await
            .map_err(map_error)?;
        require_accepted(&response)?;
        Ok(GatewayQueryResult {
            provider_ref: provider_ref.into(),
            status: map_status(&response.string("trans_stat")),
            amount: callback_amount(&response.string("trans_amt")),
            currency: "CNY".into(),
            paid_at: None,
            payload: response.data,
        })
    }

    fn verify_callback(
        &self,
        raw: &ChannelConfig,
        form: &FormMap,
        _body: &[u8],
    ) -> Result<GatewayCallbackResult, GatewayError> {
        let (config, client) = self.client(raw)?;
        let sign = form_raw(form, "sign");
        let resp_data = form_raw(form, "resp_data");
        let notify = client.verify_notify(&sign, &resp_data).map_err(map_error)?;
        if notify.string("huifu_id") != config.huifu_id.trim() {
            return Err(GatewayError::AuthFailed(
                "huifu notification merchant mismatch".into(),
            ));
        }
        let order_no = notify.string("req_seq_id");
        let req_date = notify.string("req_date");
        Ok(GatewayCallbackResult {
            provider_ref: locator(&req_date, &order_no),
            order_no,
            status: map_status(&notify.string("trans_stat")),
            amount: callback_amount(&notify.string("trans_amt")),
            currency: "CNY".into(),
            paid_at: None,
            payload: notify.data,
        })
    }

    async fn refund_payment(
        &self,
        raw: &ChannelConfig,
        input: &GatewayRefundInput,
    ) -> Result<GatewayRefundResult, GatewayError> {
        let (config, client) = self.client(raw)?;
        let (org_date, org_sequence) = split_locator(&input.provider_ref)?;
        let date = if input.request_date.trim().is_empty() {
            self.date()
        } else {
            input.request_date.trim().to_owned()
        };
        let notify_url = if input.notify_url.trim().is_empty() {
            config.notify_url
        } else {
            input.notify_url.clone()
        };
        let response = client
            .refund(&RefundRequest {
                req_date: date.clone(),
                req_seq_id: input.refund_no.clone(),
                org_req_date: org_date.into(),
                org_req_seq_id: org_sequence.into(),
                ord_amt: input.amount.to_string(),
                notify_url,
                remark: input.remark.clone(),
                client_ip: input.client_ip.clone(),
            })
            .await
            .map_err(map_error)?;
        require_accepted(&response)?;
        Ok(GatewayRefundResult {
            provider_ref: locator(&date, &input.refund_no),
            status: map_status(&response.string("trans_stat")).or(Some(PaymentStatus::Pending)),
            payload: response.data,
        })
    }

    async fn query_refund(
        &self,
        raw: &ChannelConfig,
        provider_ref: &str,
    ) -> Result<GatewayRefundResult, GatewayError> {
        let (_, client) = self.client(raw)?;
        let (org_date, org_sequence) = split_locator(provider_ref)?;
        let response = client
            .query_refund(&RefundQueryRequest {
                req_date: self.date(),
                req_seq_id: self.query_no("ZSRQ"),
                org_req_date: org_date.into(),
                org_req_seq_id: org_sequence.into(),
            })
            .await
            .map_err(map_error)?;
        require_accepted(&response)?;
        Ok(GatewayRefundResult {
            provider_ref: provider_ref.into(),
            status: map_status(&response.string("trans_stat")),
            payload: response.data,
        })
    }

    async fn query_trade_bill(
        &self,
        raw: &ChannelConfig,
        file_date: &str,
    ) -> Result<GatewayTradeBillQuery, GatewayError> {
        let (_, client) = self.client(raw)?;
        let result = client
            .query_trade_bill(&TradeBillQueryRequest {
                req_date: self.date(),
                req_seq_id: self.query_no("ZSBQ"),
                file_date: file_date.trim().to_owned(),
            })
            .await
            .map_err(map_error)?;
        Ok(GatewayTradeBillQuery {
            files: result
                .files
                .into_iter()
                .map(|file| GatewayTradeBillFile {
                    file_date: file.file_date,
                    file_id: file.file_id,
                    file_name: file.file_name,
                })
                .collect(),
            tasks: result
                .tasks
                .into_iter()
                .map(|task| GatewayTradeBillTask {
                    data_date: task.data_date,
                    task_stat: task.task_stat,
                    task_start_time: task.task_start_time,
                    task_end_time: task.task_end_time,
                })
                .collect(),
        })
    }

    async fn download_trade_bill(
        &self,
        raw: &ChannelConfig,
        file_date: &str,
        file_id: &str,
    ) -> Result<GatewayTradeBillDownload, GatewayError> {
        let (_, client) = self.client(raw)?;
        let result = client
            .query_trade_bill(&TradeBillQueryRequest {
                req_date: self.date(),
                req_seq_id: self.query_no("ZSBQ"),
                file_date: file_date.trim().to_owned(),
            })
            .await
            .map_err(map_error)?;
        let file = result
            .files
            .iter()
            .find(|file| file.file_id == file_id.trim())
            .ok_or_else(|| GatewayError::response("huifu trade bill file not found"))?;
        let body = client.download_trade_bill(file).await.map_err(map_error)?;
        Ok(GatewayTradeBillDownload {
            file_name: file.file_name.clone(),
            body,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::{GatewayError, HuifuGateway, huifu_request_type, require_accepted};
    use huifu_pay::ApiResponse;
    use serde_json::{Map, Value, json};
    use zs_domain::payment::channel::ChannelConfig;
    use zs_domain::payment::gateway::{GatewayRefundInput, PaymentGateway};
    use zs_domain::payment::types::PaymentStatus;
    use zs_shared::money::Amount;

    use crate::payment::http::{HttpResponse, MockTransport};
    use crate::payment::test_support::{PRIV_PEM, PUB_PEM, env_with};

    fn response(code: &str, desc: &str) -> ApiResponse {
        let mut data = Map::new();
        data.insert("resp_code".to_owned(), Value::String(code.to_owned()));
        data.insert("resp_desc".to_owned(), Value::String(desc.to_owned()));
        ApiResponse { data }
    }

    #[test]
    fn selects_h5_for_mobile_browsers_and_pc_by_default() {
        for user_agent in [
            "Mozilla/5.0 (iPhone; CPU iPhone OS 17_0 like Mac OS X)",
            "Mozilla/5.0 (Linux; Android 14; Pixel 8)",
            "Mozilla/5.0 (Linux; Android 14) MicroMessenger/8.0",
        ] {
            assert_eq!(huifu_request_type(user_agent), "M");
        }
        assert_eq!(
            huifu_request_type("Mozilla/5.0 (Macintosh; Intel Mac OS X 14_0) Safari/605.1"),
            "P"
        );
        assert_eq!(huifu_request_type(""), "P");
    }

    #[test]
    fn identifies_huifu_unified_cashier_permission_rejection() {
        let result = require_accepted(&response("00000009", "商户暂未开通支付统一收银台权限"));
        assert!(matches!(
            result,
            Err(GatewayError::ProviderPermissionMissing(_))
        ));

        let other = require_accepted(&response("OTHER", "some other rejection"));
        assert!(matches!(other, Err(GatewayError::ResponseInvalid(_))));
    }

    #[tokio::test]
    async fn refund_uses_persisted_request_date_after_midnight_retry() {
        let captured = Arc::new(Mutex::new(None::<Value>));
        let captured_request = captured.clone();
        let transport = MockTransport::new(move |request| {
            assert!(request.url.ends_with(huifu_pay::REFUND_PATH));
            let body: Value =
                serde_json::from_slice(&request.body).map_err(|error| error.to_string())?;
            *captured_request.lock().map_err(|error| error.to_string())? = Some(body.clone());
            let data = json!({
                "resp_code": "00000000",
                "resp_desc": "success",
                "trans_stat": "P"
            });
            let sign = huifu_pay::sign_value(PRIV_PEM, &data).map_err(|error| error.to_string())?;
            Ok(HttpResponse::new(
                200,
                json!({"data": data, "sign": sign}).to_string(),
            ))
        });
        let gateway = HuifuGateway::new(env_with(transport));
        let config: ChannelConfig = serde_json::from_value(json!({
            "sys_id": "SYS-1",
            "product_id": "PROD-1",
            "huifu_id": "HU-1",
            "merchant_private_key": PRIV_PEM,
            "huifu_public_key": PUB_PEM,
            "project_id": "PROJECT-1",
            "notify_url": "https://shop.example.com/api/v1/payments/callback",
            "return_url": "https://shop.example.com/pay"
        }))
        .map_err(|error| error.to_string())
        .expect("test config is valid");
        let result = gateway
            .refund_payment(
                &config,
                &GatewayRefundInput {
                    provider_ref: "20260930:PAYMENT-1".into(),
                    request_date: "20261001".into(),
                    refund_no: "REFUND-1".into(),
                    amount: Amount::new("0.01".parse().expect("valid amount")),
                    notify_url: String::new(),
                    remark: "midnight retry".into(),
                    client_ip: String::new(),
                },
            )
            .await
            .expect("refund request is accepted");

        assert_eq!(result.status, Some(PaymentStatus::Pending));
        let body = captured
            .lock()
            .expect("captured request")
            .clone()
            .expect("body");
        assert_eq!(body["data"]["req_date"], "20261001");
        assert_eq!(body["data"]["org_req_date"], "20260930");
        assert_eq!(body["data"]["req_seq_id"], "REFUND-1");
    }
}
