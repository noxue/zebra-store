//! Notification center use cases (port of `modules/notification/application`):
//! queueing events, dispatching them to email / Telegram / Feishu, test sends,
//! delivery logs and the periodic inventory / payment alert check.

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Duration;
use serde_json::Value;
use zs_domain::notify::center::{
    self, DispatchPayload, NotifyEvent, Rendered, Vars, channels, events,
};
use zs_domain::notify::log::{
    LogFilter, NewNotificationLog, NotificationLog, NotificationLogRepo, status,
};
use zs_domain::notify::ports::{
    AlertSource, EmailSender, FeishuSender, KEY_SEND_FAILED, MSG_CONFIG_INVALID, Notifier,
    OnceGuard, TelegramMessage, TelegramSender,
};
use zs_domain::queue::{JobQueue, NewJob, kinds};
use zs_domain::settings::schema::login::TelegramAuthSetting;
use zs_domain::settings::schema::notification::NotificationCenterSetting;
use zs_domain::settings::schema::storefront::DashboardSetting;
use zs_domain::settings::{SettingsStore, keys as setting_keys};
use zs_domain::{Error, Result};
use zs_shared::clock::Clock;
use zs_shared::page::Page;

/// Error of an invalid notification configuration (rendered verbatim as a 400 like the original).
pub fn config_invalid() -> Error {
    Error::bad_request(MSG_CONFIG_INVALID)
}

fn send_failed(detail: impl Into<String>) -> Error {
    Error::internal_msg(detail.into()).or_internal(KEY_SEND_FAILED)
}

/// Queues events as `notification:dispatch` jobs (the [`Notifier`] other modules use).
#[derive(Clone)]
pub struct QueueNotifier {
    queue: Arc<dyn JobQueue>,
}

impl std::fmt::Debug for QueueNotifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("QueueNotifier")
    }
}

impl QueueNotifier {
    pub fn new(queue: Arc<dyn JobQueue>) -> Self {
        Self { queue }
    }
}

#[async_trait]
impl Notifier for QueueNotifier {
    async fn notify(&self, event: NotifyEvent) -> Result<()> {
        let payload = event.into_payload().ok_or_else(Error::invalid)?;
        let job = NewJob::new(kinds::NOTIFICATION_DISPATCH, payload)?
            .attempts(center::DISPATCH_MAX_RETRY);
        self.queue.enqueue(job).await
    }
}

/// Adapters of [`NotificationService`].
#[derive(Clone)]
pub struct CenterDeps {
    pub settings: Arc<dyn SettingsStore>,
    pub email: Arc<dyn EmailSender>,
    pub telegram: Arc<dyn TelegramSender>,
    pub feishu: Arc<dyn FeishuSender>,
    pub logs: Arc<dyn NotificationLogRepo>,
    pub guard: Arc<dyn OnceGuard>,
    pub alerts: Arc<dyn AlertSource>,
    pub clock: Arc<dyn Clock>,
    /// `telegram_auth.bot_token` from the configuration file (used when never saved).
    pub fallback_bot_token: String,
}

impl std::fmt::Debug for CenterDeps {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("CenterDeps")
    }
}

/// Test-send request (original `TestSendInput`).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct TestSend {
    pub channel: String,
    pub target: String,
    pub scene: String,
    pub locale: String,
    pub variables: Vars,
}

/// One delivery to log.
struct Attempt<'a> {
    payload: &'a DispatchPayload,
    channel: &'a str,
    recipient: &'a str,
    locale: &'a str,
    rendered: &'a Rendered,
    vars: &'a Vars,
    is_test: bool,
    result: &'a Result<()>,
}

/// Notification center service (cheap to clone).
#[derive(Clone)]
pub struct NotificationService {
    d: CenterDeps,
}

impl std::fmt::Debug for NotificationService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("NotificationService")
    }
}

impl NotificationService {
    pub fn new(deps: CenterDeps) -> Self {
        Self { d: deps }
    }

    async fn setting(&self) -> Result<NotificationCenterSetting> {
        let raw = self
            .d
            .settings
            .get(setting_keys::NOTIFICATION_CENTER_CONFIG)
            .await?;
        Ok(
            NotificationCenterSetting::decode(raw.as_ref(), NotificationCenterSetting::default())
                .normalized(),
        )
    }

    async fn dashboard(&self) -> Result<DashboardSetting> {
        let raw = self.d.settings.get(setting_keys::DASHBOARD_CONFIG).await?;
        Ok(DashboardSetting::decode(
            raw.as_ref(),
            DashboardSetting::default(),
        ))
    }

    /// Bot token of the Telegram login settings (original `resolveBotToken`).
    async fn bot_token(&self) -> Result<String> {
        let raw = self
            .d
            .settings
            .get(setting_keys::TELEGRAM_AUTH_CONFIG)
            .await?;
        let fallback = TelegramAuthSetting {
            bot_token: self.d.fallback_bot_token.clone(),
            ..TelegramAuthSetting::default()
        };
        Ok(TelegramAuthSetting::decode(raw.as_ref(), fallback)
            .bot_token
            .trim()
            .to_owned())
    }

    async fn send_telegram(&self, chat_id: &str, text: &str) -> Result<()> {
        let token = self.bot_token().await?;
        if token.is_empty() {
            return Err(config_invalid());
        }
        let message = TelegramMessage {
            chat_id: chat_id.to_owned(),
            text: text.to_owned(),
            disable_web_page_preview: true,
            ..TelegramMessage::default()
        };
        self.d.telegram.send(&token, &message).await
    }

    async fn record(&self, a: Attempt<'_>) {
        let channel = a.channel.trim().to_lowercase();
        let recipient = a.recipient.trim();
        if channel.is_empty() || recipient.is_empty() {
            return;
        }
        let (log_status, error_message) = match a.result {
            Ok(()) => (status::SUCCESS, String::new()),
            Err(e) => (status::FAILED, e.to_string()),
        };
        let log = NewNotificationLog {
            event_type: center::normalize_event(&a.payload.event_type),
            biz_type: a.payload.biz_type.trim().to_lowercase(),
            biz_id: a.payload.biz_id,
            channel,
            recipient: recipient.to_owned(),
            locale: a.locale.trim().to_owned(),
            title: a.rendered.title.trim().to_owned(),
            body: a.rendered.body.trim().to_owned(),
            status: log_status.to_owned(),
            error_message: error_message.trim().to_owned(),
            is_test: a.is_test,
            variables: Value::Object(a.vars.clone()),
            created_at: self.d.clock.now(),
        };
        if let Err(error) = self.d.logs.create(&log).await {
            tracing::warn!(%error, event_type = %log.event_type, "notification log record failed");
        }
    }

    /// Handles a `notification:dispatch` job. Errors trigger a retry.
    pub async fn dispatch(&self, payload: &DispatchPayload) -> Result<()> {
        let event = center::normalize_event(&payload.event_type);
        if !center::is_supported_event(&event) {
            // Unknown events can never succeed: drop them instead of retrying.
            tracing::warn!(event_type = %payload.event_type, "notification event invalid");
            return Ok(());
        }
        let setting = self.setting().await?;
        if !center::scene_enabled(&setting.scenes, &event) {
            return Ok(());
        }
        if event == events::EXCEPTION_ALERT_CHECK {
            return self.alert_check(&setting, payload).await;
        }
        self.dispatch_single(&setting, payload).await
    }

    async fn dispatch_single(
        &self,
        setting: &NotificationCenterSetting,
        payload: &DispatchPayload,
    ) -> Result<()> {
        if !payload.force {
            let ttl = if setting.dedupe_ttl_seconds > 0 {
                setting.dedupe_ttl_seconds
            } else {
                center::DEFAULT_DEDUPE_TTL_SECONDS
            };
            match self
                .d
                .guard
                .acquire(&center::dedupe_key(payload), ttl)
                .await
            {
                Ok(false) => return Ok(()),
                Ok(true) => {}
                Err(error) => tracing::warn!(%error, "notification dedupe failed"),
            }
        }
        let locale = center::resolve_locale(&payload.locale, &setting.default_locale);
        let template = center::template_for(&setting.templates, &payload.event_type, &locale);
        let vars = center::template_variables(payload, self.d.clock.now());
        let rendered = center::render(template, &vars, center::FALLBACK_TITLE);
        let text = center::compose_plain_text(&rendered.title, &rendered.body);

        let mut first_error: Option<Error> = None;
        let c = &setting.channels;
        let mut deliveries: Vec<(&str, &String)> = Vec::new();
        if c.email.enabled {
            deliveries.extend(c.email.recipients.iter().map(|r| (channels::EMAIL, r)));
        }
        if c.telegram.enabled {
            deliveries.extend(
                c.telegram
                    .recipients
                    .iter()
                    .map(|r| (channels::TELEGRAM, r)),
            );
        }
        if c.feishu.enabled {
            deliveries.extend(c.feishu.recipients.iter().map(|r| (channels::FEISHU, r)));
        }
        for (channel, recipient) in deliveries {
            let result = match channel {
                channels::EMAIL => {
                    self.d
                        .email
                        .send(recipient, &rendered.title, &rendered.body)
                        .await
                }
                channels::TELEGRAM => self.send_telegram(recipient, &text).await,
                _ => {
                    let f = &c.feishu;
                    self.d
                        .feishu
                        .send(
                            &f.app_id,
                            &f.app_secret,
                            &f.receive_id_type,
                            recipient,
                            &text,
                        )
                        .await
                }
            };
            self.record(Attempt {
                payload,
                channel,
                recipient,
                locale: &locale,
                rendered: &rendered,
                vars: &vars,
                is_test: false,
                result: &result,
            })
            .await;
            if let Err(error) = result {
                tracing::warn!(%error, channel, recipient = %recipient, event_type = %payload.event_type, "notification send failed");
                first_error.get_or_insert(error);
            }
        }
        match first_error {
            Some(e) => Err(send_failed(e.to_string())),
            None => Ok(()),
        }
    }

    async fn alert_check(
        &self,
        setting: &NotificationCenterSetting,
        payload: &DispatchPayload,
    ) -> Result<()> {
        let dashboard = self.dashboard().await?;
        let mut first_error: Option<Error> = None;
        let rows = self
            .d
            .alerts
            .inventory_alerts(dashboard.alert.low_stock_threshold)
            .await?;
        for item in
            center::build_inventory_alert_payloads(setting, &dashboard.alert, payload, &rows)
        {
            if let Some(kind) = center::resolve_inventory_alert_type_key(&item.data) {
                let key = format!("notification:inventory_interval:{kind}");
                match self
                    .d
                    .guard
                    .acquire(&key, setting.inventory_alert_interval_seconds)
                    .await
                {
                    Ok(false) => continue,
                    Ok(true) => {}
                    Err(error) => {
                        tracing::warn!(%error, "notification inventory alert interval failed")
                    }
                }
            }
            if let Err(e) = self.dispatch_single(setting, &item).await {
                first_error.get_or_insert(e);
            }
        }
        let now = self.d.clock.now();
        let start = now - Duration::seconds(setting.payment_order_alert_check_interval_seconds);
        let counts = self.d.alerts.payment_order_counts(start, now).await?;
        for item in
            center::build_payment_order_alert_payloads(setting, &dashboard.alert, payload, counts)
        {
            if let Some(kind) = center::resolve_payment_order_alert_type_key(&item.data) {
                let key = format!("notification:payment_order_interval:{kind}");
                match self
                    .d
                    .guard
                    .acquire(&key, setting.payment_order_alert_interval_seconds)
                    .await
                {
                    Ok(false) => continue,
                    Ok(true) => {}
                    Err(error) => {
                        tracing::warn!(%error, "notification payment alert interval failed")
                    }
                }
            }
            if let Err(e) = self.dispatch_single(setting, &item).await {
                first_error.get_or_insert(e);
            }
        }
        first_error.map_or(Ok(()), Err)
    }

    /// Runs the periodic alert check (scene switch honoured).
    pub async fn run_alert_check(&self) -> Result<()> {
        self.dispatch(&center::alert_check_payload()).await
    }

    /// Sends a rendered test message to one target and logs it (original `SendTest`).
    pub async fn send_test(&self, input: TestSend) -> Result<()> {
        let channel = input.channel.trim().to_lowercase();
        let target = input.target.trim().to_owned();
        if channel.is_empty() || target.is_empty() {
            return Err(config_invalid());
        }
        if !channels::ALL.contains(&channel.as_str()) {
            return Err(config_invalid());
        }
        let setting = self.setting().await?;
        let scene = match input.scene.trim().to_lowercase() {
            s if s.is_empty() => events::EXCEPTION_ALERT.to_owned(),
            s => s,
        };
        let locale = center::resolve_locale(&input.locale, &setting.default_locale);
        let template = center::template_for(&setting.templates, &scene, &locale);
        let mut vars = input.variables;
        for (k, v) in center::test_variables(&scene, &locale) {
            vars.entry(k).or_insert(v);
        }
        vars.insert("event_type".into(), scene.clone().into());
        let message = vars
            .get("message")
            .map(center::value_text)
            .unwrap_or_default();
        vars.insert(
            "message".into(),
            if message.is_empty() {
                "test message".into()
            } else {
                message
            }
            .into(),
        );
        let rendered = center::render(template, &vars, center::TEST_FALLBACK_TITLE);
        let text = center::compose_plain_text(&rendered.title, &rendered.body);
        let result = match channel.as_str() {
            channels::EMAIL => {
                self.d
                    .email
                    .send(&target, &rendered.title, &rendered.body)
                    .await
            }
            channels::TELEGRAM => self.send_telegram(&target, &text).await,
            _ => {
                let f = &setting.channels.feishu;
                self.d
                    .feishu
                    .send(&f.app_id, &f.app_secret, &f.receive_id_type, &target, &text)
                    .await
            }
        };
        let payload = DispatchPayload {
            event_type: scene,
            ..DispatchPayload::default()
        };
        self.record(Attempt {
            payload: &payload,
            channel: &channel,
            recipient: &target,
            locale: &locale,
            rendered: &rendered,
            vars: &vars,
            is_test: true,
            result: &result,
        })
        .await;
        result.map_err(|e| {
            if e.key() == MSG_CONFIG_INVALID {
                e
            } else {
                send_failed(e.to_string())
            }
        })
    }

    /// Admin log list.
    pub async fn list_logs(&self, filter: &LogFilter) -> Result<Page<NotificationLog>> {
        self.d
            .logs
            .list(filter)
            .await
            .map_err(|e| e.or_internal("error.config_fetch_failed"))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use chrono::{DateTime, Utc};
    use serde_json::json;
    use zs_domain::notify::center::{InventoryAlertRow, PaymentOrderAlertCounts, alert_types};
    use zs_shared::clock::FixedClock;
    use zs_shared::page::Page;

    use super::*;
    use crate::notify::guard::MemoryGuard;

    #[derive(Default)]
    struct Settings(Mutex<HashMap<String, Value>>);

    #[async_trait]
    impl SettingsStore for Settings {
        async fn get(&self, key: &str) -> Result<Option<Value>> {
            Ok(self.0.lock().unwrap().get(key).cloned())
        }
        async fn set(&self, key: &str, value: &Value) -> Result<()> {
            self.0.lock().unwrap().insert(key.into(), value.clone());
            Ok(())
        }
    }

    /// Records every delivery as `(channel, recipient, text)`; fails recipients starting with `fail`.
    #[derive(Default)]
    struct Sent(Mutex<Vec<(String, String, String)>>);

    impl Sent {
        fn push(&self, channel: &str, to: &str, text: &str) -> Result<()> {
            self.0
                .lock()
                .unwrap()
                .push((channel.into(), to.into(), text.into()));
            if to.starts_with("fail") {
                Err(Error::internal_msg("boom"))
            } else {
                Ok(())
            }
        }
        fn all(&self) -> Vec<(String, String, String)> {
            self.0.lock().unwrap().clone()
        }
    }

    struct Mail(Arc<Sent>);
    #[async_trait]
    impl EmailSender for Mail {
        async fn send(&self, to: &str, subject: &str, body: &str) -> Result<()> {
            self.0.push("email", to, &format!("{subject}|{body}"))
        }
    }

    struct Tg(Arc<Sent>);
    #[async_trait]
    impl TelegramSender for Tg {
        async fn send(&self, token: &str, m: &TelegramMessage) -> Result<()> {
            self.0
                .push("telegram", &m.chat_id, &format!("{token}|{}", m.text))
        }
    }

    struct Fs(Arc<Sent>);
    #[async_trait]
    impl FeishuSender for Fs {
        async fn send(
            &self,
            app_id: &str,
            _s: &str,
            kind: &str,
            to: &str,
            text: &str,
        ) -> Result<()> {
            self.0
                .push("feishu", to, &format!("{app_id}|{kind}|{text}"))
        }
    }

    #[derive(Default)]
    struct Logs(Mutex<Vec<NewNotificationLog>>);
    #[async_trait]
    impl NotificationLogRepo for Logs {
        async fn create(&self, log: &NewNotificationLog) -> Result<()> {
            self.0.lock().unwrap().push(log.clone());
            Ok(())
        }
        async fn list(&self, _f: &LogFilter) -> Result<Page<NotificationLog>> {
            Ok(Page {
                items: vec![],
                total: 0,
            })
        }
    }

    #[derive(Default)]
    struct Alerts {
        rows: Vec<InventoryAlertRow>,
        counts: PaymentOrderAlertCounts,
        windows: Mutex<Vec<(DateTime<Utc>, DateTime<Utc>)>>,
    }
    #[async_trait]
    impl AlertSource for Alerts {
        async fn inventory_alerts(&self, _low: i64) -> Result<Vec<InventoryAlertRow>> {
            Ok(self.rows.clone())
        }
        async fn payment_order_counts(
            &self,
            s: DateTime<Utc>,
            e: DateTime<Utc>,
        ) -> Result<PaymentOrderAlertCounts> {
            self.windows.lock().unwrap().push((s, e));
            Ok(self.counts)
        }
    }

    struct Fixture {
        svc: NotificationService,
        sent: Arc<Sent>,
        logs: Arc<Logs>,
        settings: Arc<Settings>,
        alerts: Arc<Alerts>,
    }

    fn now() -> DateTime<Utc> {
        DateTime::parse_from_rfc3339("2026-05-01T00:00:00Z")
            .unwrap()
            .with_timezone(&Utc)
    }

    fn fixture(config: Value, alerts: Alerts) -> Fixture {
        let settings = Arc::new(Settings::default());
        settings
            .0
            .lock()
            .unwrap()
            .insert(setting_keys::NOTIFICATION_CENTER_CONFIG.into(), config);
        settings.0.lock().unwrap().insert(
            setting_keys::TELEGRAM_AUTH_CONFIG.into(),
            json!({"bot_token": "123:ABC"}),
        );
        let sent = Arc::new(Sent::default());
        let logs = Arc::new(Logs::default());
        let alerts = Arc::new(alerts);
        let clock: Arc<dyn Clock> = Arc::new(FixedClock(now()));
        let svc = NotificationService::new(CenterDeps {
            settings: settings.clone(),
            email: Arc::new(Mail(sent.clone())),
            telegram: Arc::new(Tg(sent.clone())),
            feishu: Arc::new(Fs(sent.clone())),
            logs: logs.clone(),
            guard: Arc::new(MemoryGuard::new(clock.clone())),
            alerts: alerts.clone(),
            clock,
            fallback_bot_token: String::new(),
        });
        Fixture {
            svc,
            sent,
            logs,
            settings,
            alerts,
        }
    }

    fn all_channels() -> Value {
        json!({
            "channels": {
                "email": {"enabled": true, "recipients": ["ops@example.com", "fail@example.com"]},
                "telegram": {"enabled": true, "recipients": ["-1001234567"]},
                "feishu": {"enabled": true, "app_id": "cli_1", "app_secret": "s", "receive_id_type": "chat_id", "recipients": ["oc_1"]}
            }
        })
    }

    fn order_paid() -> DispatchPayload {
        DispatchPayload {
            event_type: "order_paid_success".into(),
            biz_type: "order".into(),
            biz_id: 9,
            locale: "en-US".into(),
            data: json!({"order_no": "DJ9", "amount": "1.00", "currency": "USD"})
                .as_object()
                .cloned()
                .unwrap(),
            ..DispatchPayload::default()
        }
    }

    #[tokio::test]
    async fn dispatch_sends_every_channel_and_logs() {
        let f = fixture(all_channels(), Alerts::default());
        let err = f.svc.dispatch(&order_paid()).await.unwrap_err();
        assert_eq!(
            err.key(),
            KEY_SEND_FAILED,
            "one failing recipient fails the job"
        );
        let sent = f.sent.all();
        assert_eq!(sent.len(), 4);
        assert_eq!(sent[0].0, "email");
        assert!(sent[0].2.starts_with("Order Payment Succeeded|"));
        assert!(sent[0].2.contains("Order No: DJ9"));
        assert_eq!(sent[2].0, "telegram");
        assert!(
            sent[2]
                .2
                .starts_with("123:ABC|Order Payment Succeeded\n\nCustomer:")
        );
        assert_eq!(sent[3], ("feishu".into(), "oc_1".into(), sent[3].2.clone()));
        assert!(
            sent[3]
                .2
                .starts_with("cli_1|chat_id|Order Payment Succeeded")
        );
        let logs = f.logs.0.lock().unwrap().clone();
        assert_eq!(logs.len(), 4);
        assert_eq!(logs[0].status, "success");
        assert_eq!(logs[1].status, "failed");
        assert!(logs[1].error_message.contains("boom"));
        assert_eq!(logs[0].biz_id, 9);
        assert_eq!(logs[0].locale, "en-US");
        assert_eq!(logs[0].variables["order_no"], "DJ9");
        assert!(!logs[0].is_test);
    }

    #[tokio::test]
    async fn duplicate_event_is_suppressed_unless_forced() {
        let f = fixture(
            json!({"channels": {"telegram": {"enabled": true, "recipients": ["-1001234567"]}}}),
            Alerts::default(),
        );
        f.svc.dispatch(&order_paid()).await.unwrap();
        f.svc.dispatch(&order_paid()).await.unwrap();
        assert_eq!(f.sent.all().len(), 1);
        let forced = DispatchPayload {
            force: true,
            ..order_paid()
        };
        f.svc.dispatch(&forced).await.unwrap();
        assert_eq!(f.sent.all().len(), 2);
    }

    #[tokio::test]
    async fn disabled_scene_or_unknown_event_sends_nothing() {
        let mut cfg = all_channels();
        cfg["scenes"] = json!({"order_paid_success": false});
        let f = fixture(cfg, Alerts::default());
        f.svc.dispatch(&order_paid()).await.unwrap();
        let unknown = DispatchPayload {
            event_type: "nope".into(),
            ..order_paid()
        };
        f.svc.dispatch(&unknown).await.unwrap();
        assert!(f.sent.all().is_empty());
    }

    #[tokio::test]
    async fn telegram_without_bot_token_is_logged_as_failure() {
        let f = fixture(
            json!({"channels": {"telegram": {"enabled": true, "recipients": ["-1001234567"]}}}),
            Alerts::default(),
        );
        f.settings
            .0
            .lock()
            .unwrap()
            .remove(setting_keys::TELEGRAM_AUTH_CONFIG);
        assert!(f.svc.dispatch(&order_paid()).await.is_err());
        assert!(f.sent.all().is_empty());
        let logs = f.logs.0.lock().unwrap().clone();
        assert_eq!(logs[0].status, "failed");
        assert!(logs[0].error_message.contains(MSG_CONFIG_INVALID));
    }

    fn low_stock_rows() -> Vec<InventoryAlertRow> {
        vec![InventoryAlertRow {
            product_id: 1,
            sku_id: 2,
            product_title: json!({"zh-CN": "商品"}),
            fulfillment_type: "manual".into(),
            alert_type: alert_types::LOW_STOCK_PRODUCTS.into(),
            available_stock: 1,
            ..InventoryAlertRow::default()
        }]
    }

    // NTF-04 ①③ / NTF-08: alert intervals are keyed by alert type.
    #[tokio::test]
    async fn ntf04_ntf08_alert_intervals_limit_repeats() {
        let cfg = json!({
            "channels": {"telegram": {"enabled": true, "recipients": ["-1001234567"]}},
            "inventory_alert_interval_seconds": 3600,
            "payment_order_alert_interval_seconds": 600,
            "payment_order_alert_check_interval_seconds": 900,
            "dedupe_ttl_seconds": 30
        });
        let alerts = Alerts {
            rows: low_stock_rows(),
            counts: PaymentOrderAlertCounts {
                pending_payment_orders: 25,
                payments_failed: 0,
            },
            ..Alerts::default()
        };
        let f = fixture(cfg, alerts);
        f.svc.run_alert_check().await.unwrap();
        let first = f.sent.all();
        assert_eq!(first.len(), 2, "one low-stock and one pending-orders alert");
        assert!(first[0].2.contains("低库存商品"));
        assert!(first[1].2.contains("待支付订单"));
        // Second run inside both intervals: nothing new is sent.
        f.svc.run_alert_check().await.unwrap();
        assert_eq!(f.sent.all().len(), 2);
        // NTF-08: the counting window is `now - check_interval .. now`.
        let windows = f.alerts.windows.lock().unwrap().clone();
        assert_eq!(windows[0], (now() - Duration::seconds(900), now()));
    }

    #[tokio::test]
    async fn alert_check_respects_exception_scene() {
        let cfg = json!({
            "channels": {"telegram": {"enabled": true, "recipients": ["-1001234567"]}},
            "scenes": {"exception_alert": false}
        });
        let f = fixture(
            cfg,
            Alerts {
                rows: low_stock_rows(),
                ..Alerts::default()
            },
        );
        f.svc.run_alert_check().await.unwrap();
        assert!(f.sent.all().is_empty());
    }

    #[tokio::test]
    async fn test_send_renders_sample_and_logs_as_test() {
        let f = fixture(all_channels(), Alerts::default());
        f.svc
            .send_test(TestSend {
                channel: " FEISHU ".into(),
                target: " oc_demo ".into(),
                scene: "wallet_recharge_success".into(),
                locale: "en-US".into(),
                variables: Vars::new(),
            })
            .await
            .unwrap();
        let sent = f.sent.all();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].1, "oc_demo");
        assert!(sent[0].2.contains("Recharge No: RC202603230001"));
        let logs = f.logs.0.lock().unwrap().clone();
        assert!(logs[0].is_test);
        assert_eq!(logs[0].event_type, "wallet_recharge_success");

        let err = f
            .svc
            .send_test(TestSend {
                channel: "email".into(),
                target: " ".into(),
                ..TestSend::default()
            })
            .await
            .unwrap_err();
        assert_eq!(err.key(), MSG_CONFIG_INVALID);
        let err = f
            .svc
            .send_test(TestSend {
                channel: "email".into(),
                target: "fail@example.com".into(),
                ..TestSend::default()
            })
            .await
            .unwrap_err();
        assert_eq!(err.key(), KEY_SEND_FAILED);
    }

    #[tokio::test]
    async fn queue_notifier_rejects_unknown_events() {
        #[derive(Default)]
        struct Q(Mutex<Vec<NewJob>>);
        #[async_trait]
        impl JobQueue for Q {
            async fn enqueue(&self, job: NewJob) -> Result<()> {
                self.0.lock().unwrap().push(job);
                Ok(())
            }
        }
        let q = Arc::new(Q::default());
        let n = QueueNotifier::new(q.clone());
        assert!(
            n.notify(NotifyEvent {
                event_type: "x".into(),
                ..NotifyEvent::default()
            })
            .await
            .is_err()
        );
        n.notify(NotifyEvent {
            event_type: " Order_Paid_Success ".into(),
            biz_id: 3,
            ..NotifyEvent::default()
        })
        .await
        .unwrap();
        let jobs = q.0.lock().unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].kind, kinds::NOTIFICATION_DISPATCH);
        assert_eq!(jobs[0].max_attempts, 5);
        assert_eq!(jobs[0].payload["event_type"], "order_paid_success");
    }
}
