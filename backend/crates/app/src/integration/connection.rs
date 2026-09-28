//! Site connections: CRUD with encrypted secret, adapter selection and negotiation
//! (handshake, capabilities, push-event registration), ping, status, pricing
//! re-application.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use zs_domain::catalog::product::JsonMap;
use zs_domain::integration::adapter::{
    AdapterMeta, Capabilities, Capability, ConnectionCode, FieldKind, HandshakeInfo,
    SupplierAdapter,
};
use zs_domain::integration::connection::{
    ConnectionInput, ConnectionRepo, ConnectionState, ConnectionStatus, Endpoint, SiteConnection,
    SyncMode, WebhookStatus, pricing_changed,
};
use zs_domain::integration::keys;
use zs_domain::integration::mapping::MappingRepo;
use zs_domain::integration::protocol::{
    PingInfo, UpstreamClient, UpstreamConnector, UpstreamError,
};
use zs_domain::{Error, Id, Result};
use zs_shared::clock::Clock;
use zs_shared::crypto::Cipher;
use zs_shared::page::{Page, PageRequest};

/// Site connection use cases.
#[derive(Clone)]
pub struct ConnectionService {
    repo: Arc<dyn ConnectionRepo>,
    mappings: Arc<dyn MappingRepo>,
    connector: Arc<dyn UpstreamConnector>,
    cipher: Cipher,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for ConnectionService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ConnectionService")
    }
}

pub(crate) fn not_found() -> Error {
    Error::not_found(keys::CONNECTION_NOT_FOUND)
}

/// Result of probing a supplier before a connection exists.
#[derive(Debug, Clone, PartialEq)]
pub struct Probe {
    pub handshake: HandshakeInfo,
    pub capabilities: Capabilities,
    pub meta: AdapterMeta,
}

impl ConnectionService {
    pub fn new(
        repo: Arc<dyn ConnectionRepo>,
        mappings: Arc<dyn MappingRepo>,
        connector: Arc<dyn UpstreamConnector>,
        cipher: Cipher,
        clock: Arc<dyn Clock>,
    ) -> Self {
        Self {
            repo,
            mappings,
            connector,
            cipher,
            clock,
        }
    }

    fn now(&self) -> DateTime<Utc> {
        self.clock.now()
    }

    // ------------------------------------------------------------------ adapters

    /// Registered supplier systems.
    pub fn protocols(&self) -> Vec<AdapterMeta> {
        self.connector.adapters().iter().map(|a| a.meta()).collect()
    }

    /// The adapter of a protocol id.
    pub fn adapter(&self, protocol: &str) -> Option<Arc<dyn SupplierAdapter>> {
        self.connector.adapter(protocol)
    }

    fn ensure_protocol(&self, protocol: &str) -> Result<()> {
        if self.connector.adapter(protocol).is_some() {
            Ok(())
        } else {
            Err(Error::bad_request(keys::CONNECTION_INVALID))
        }
    }

    /// Parses a connection code with the first adapter recognising its format.
    pub fn parse_code(&self, code: &str) -> Result<ConnectionCode> {
        for adapter in self.connector.adapters() {
            if let Some(parsed) = adapter.parse_connection_code(code) {
                return parsed.map_err(|reason| {
                    tracing::info!(%reason, "connection code refused");
                    Error::bad_request(keys::CONNECTION_CODE_INVALID)
                });
            }
        }
        Err(Error::bad_request(keys::CONNECTION_CODE_INVALID))
    }

    /// Handshake with a supplier that is not saved yet (admin form pre-fill).
    pub async fn probe(&self, endpoint: &Endpoint) -> std::result::Result<Probe, UpstreamError> {
        let adapter = self
            .connector
            .adapter(&endpoint.protocol)
            .ok_or_else(|| UpstreamError::Protocol("unsupported protocol".into()))?;
        let open = |ep: &Endpoint| {
            adapter
                .open(ep)
                .map_err(|e| UpstreamError::Protocol(e.to_string()))
        };
        let handshake = open(endpoint)?.handshake().await?;
        let negotiated = Endpoint {
            features: Some(handshake.features.clone()),
            ..endpoint.clone()
        };
        let capabilities = open(&negotiated)?.capabilities();
        Ok(Probe {
            handshake,
            capabilities,
            meta: adapter.meta(),
        })
    }

    // ------------------------------------------------------------------ CRUD

    /// Admin view: adapter configuration fields of kind `secret` are never returned.
    fn masked(&self, mut conn: SiteConnection) -> SiteConnection {
        if let Some(adapter) = self.connector.adapter(&conn.protocol) {
            for field in adapter.meta().fields {
                if field.kind == FieldKind::Secret && conn.state.extra.contains_key(field.key) {
                    conn.state.extra.insert(
                        field.key.to_owned(),
                        serde_json::Value::String(String::new()),
                    );
                }
            }
        }
        conn
    }

    /// Keys of the `secret` configuration fields of a protocol.
    fn secret_keys(&self, protocol: &str) -> Vec<&'static str> {
        self.connector
            .adapter(protocol)
            .map(|a| {
                a.meta()
                    .fields
                    .into_iter()
                    .filter(|f| f.kind == FieldKind::Secret)
                    .map(|f| f.key)
                    .collect()
            })
            .unwrap_or_default()
    }

    /// New adapter configuration of an update: a `secret` field sent empty (the admin
    /// view masks it) or left out keeps the stored value, like `api_secret`.
    fn merged_extra(&self, protocol: &str, stored: &JsonMap, input: &JsonMap) -> JsonMap {
        let mut out = input.clone();
        for key in self.secret_keys(protocol) {
            let blank = match out.get(key) {
                None | Some(serde_json::Value::Null) => true,
                Some(serde_json::Value::String(v)) => v.trim().is_empty(),
                Some(_) => false,
            };
            if !blank {
                continue;
            }
            match stored.get(key) {
                Some(v) => {
                    out.insert(key.to_owned(), v.clone());
                }
                None => {
                    out.remove(key);
                }
            }
        }
        out
    }

    pub async fn list(&self, page: PageRequest) -> Result<Page<SiteConnection>> {
        let page = self
            .repo
            .list(page, "")
            .await
            .map_err(|e| e.or_internal(keys::CONNECTION_FETCH_FAILED))?;
        Ok(Page {
            items: page.items.into_iter().map(|c| self.masked(c)).collect(),
            total: page.total,
        })
    }

    /// Live connection (`None` when missing).
    pub async fn find(&self, id: Id) -> Result<Option<SiteConnection>> {
        self.repo.get(id).await
    }

    /// Admin view of one connection (secret configuration fields masked).
    pub async fn get(&self, id: Id) -> Result<SiteConnection> {
        self.repo
            .get(id)
            .await
            .map_err(|e| e.or_internal(keys::CONNECTION_FETCH_FAILED))?
            .map(|c| self.masked(c))
            .ok_or_else(not_found)
    }

    /// Live connection by API key (supplier callbacks).
    pub async fn find_by_key(&self, api_key: &str) -> Result<Option<SiteConnection>> {
        self.repo.find_by_key(api_key).await
    }

    /// Creates a connection; systems with negotiable capabilities are handshaken right
    /// away (best effort: a failure leaves the connection pending).
    pub async fn create(&self, input: &ConnectionInput) -> Result<SiteConnection> {
        let new = input.validate_new()?;
        self.ensure_protocol(&new.protocol)?;
        let secret = self
            .cipher
            .encrypt(&new.api_secret_plain)
            .map_err(Error::internal)
            .map_err(|e| e.or_internal(keys::CONNECTION_CREATE_FAILED))?;
        let mut conn = self
            .repo
            .create(&new, &secret, self.now())
            .await
            .map_err(|e| e.or_internal(keys::CONNECTION_CREATE_FAILED))?;
        if let Some(extra) = &input.extra {
            conn.state.extra.clone_from(extra);
            self.repo
                .save_state(conn.id, &conn.state, self.now())
                .await
                .map_err(|e| e.or_internal(keys::CONNECTION_CREATE_FAILED))?;
        }
        self.negotiate_best_effort(&mut conn).await;
        Ok(self.masked(conn))
    }

    /// Negotiation after create / update: a failure never blocks saving, it is stored
    /// with the connection (`handshake_error`, `webhook_status`).
    async fn negotiate_best_effort(&self, conn: &mut SiteConnection) {
        match self.negotiate(conn).await {
            // A successful handshake proves the connection like a ping does.
            Ok(_) if conn.status == ConnectionStatus::Pending => {
                conn.status = ConnectionStatus::Active;
                conn.last_ping_ok = true;
                conn.last_ping_at = Some(self.now());
                if let Err(error) = self.repo.save(conn, self.now()).await {
                    tracing::warn!(%error, connection_id = conn.id, "activate connection failed");
                }
            }
            Ok(_) => {}
            Err(error) => {
                tracing::warn!(%error, connection_id = conn.id, "connection handshake failed");
            }
        }
    }

    /// Handshake + capability negotiation + push-event registration; stores the state
    /// (also on failure, with the error).
    pub async fn negotiate(&self, conn: &mut SiteConnection) -> Result<HandshakeInfo> {
        let client = self.client(conn)?;
        let info = match client.handshake().await {
            Ok(info) => info,
            Err(e) => {
                let mut state = conn.state.clone();
                state.handshake_error = e.to_string();
                state.last_handshake_at = Some(self.now());
                let push = self
                    .connector
                    .adapter(&conn.protocol)
                    .is_some_and(|a| a.meta().capabilities.has(Capability::PushEvents));
                state.webhook_status = if push {
                    WebhookStatus::Failed
                } else {
                    WebhookStatus::Unsupported
                };
                self.repo.save_state(conn.id, &state, self.now()).await?;
                conn.state = state;
                return Err(Error::new(zs_domain::ErrorKind::Internal, e.to_string()));
            }
        };
        let mut state = conn.state.clone();
        state.features.clone_from(&info.features);
        state.negotiated = true;
        state.handshake_error.clear();
        state.supplier_currency.clone_from(&info.currency);
        state.last_handshake_at = Some(self.now());
        let mut endpoint = self.endpoint(conn)?;
        endpoint.features = Some(info.features.clone());
        let client = self.connector.open(&endpoint)?;
        let caps = client.capabilities();
        state.capabilities = caps.names().into_iter().map(str::to_owned).collect();
        state.sync_mode = if caps.has(Capability::IncrementalChanges) {
            SyncMode::Incremental
        } else {
            SyncMode::Full
        };
        state.webhook_status = if !caps.has(Capability::PushEvents) {
            WebhookStatus::Unsupported
        } else if conn.callback_url.trim().is_empty() {
            WebhookStatus::None
        } else {
            match client.register_events(conn.callback_url.trim()).await {
                Ok(()) => WebhookStatus::Registered,
                Err(error) => {
                    tracing::warn!(%error, connection_id = conn.id, "push event registration failed");
                    WebhookStatus::Failed
                }
            }
        };
        self.repo.save_state(conn.id, &state, self.now()).await?;
        conn.state = state;
        Ok(info)
    }

    /// Stores the adapter state (change cursor etc.).
    pub async fn save_state(&self, id: Id, state: &ConnectionState) -> Result<()> {
        self.repo.save_state(id, state, self.now()).await
    }

    /// Updates; a changed exchange rate / markup / rounding re-prices the mapped
    /// products (best effort, UPS-05 (2)); changed access parameters renegotiate.
    pub async fn update(&self, id: Id, input: &ConnectionInput) -> Result<SiteConnection> {
        let wrap = |e: Error| e.or_internal(keys::CONNECTION_UPDATE_FAILED);
        let mut conn = self
            .repo
            .get(id)
            .await
            .map_err(wrap)?
            .ok_or_else(not_found)?;
        let before = conn.pricing();
        let protocol_before = conn.protocol.clone();
        if let Some(secret) = input.apply_update(&mut conn)? {
            conn.api_secret = self
                .cipher
                .encrypt(&secret)
                .map_err(Error::internal)
                .map_err(wrap)?;
        }
        self.ensure_protocol(&conn.protocol)?;
        if conn.protocol != protocol_before {
            // Another system: nothing negotiated with the previous one applies.
            conn.state = ConnectionState::default();
            self.repo
                .save_state(conn.id, &conn.state, self.now())
                .await?;
        }
        self.repo.save(&conn, self.now()).await.map_err(wrap)?;
        if pricing_changed(&before, &conn.pricing())
            && let Err(error) = self
                .mappings
                .reapply_pricing(conn.id, &conn.pricing(), self.now())
                .await
        {
            tracing::warn!(%error, connection_id = conn.id, "reapply markup after connection update failed");
        }
        if let Some(extra) = &input.extra {
            // `conn.state` was reset above when the protocol changed: nothing is kept then.
            conn.state.extra = self.merged_extra(&conn.protocol, &conn.state.extra, extra);
            self.repo
                .save_state(conn.id, &conn.state, self.now())
                .await
                .map_err(wrap)?;
        }
        // Renegotiate so the stored capabilities / currency / push status follow the
        // new parameters.
        self.negotiate_best_effort(&mut conn).await;
        Ok(self.masked(conn))
    }

    pub async fn delete(&self, id: Id) -> Result<()> {
        let wrap = |e: Error| e.or_internal(keys::CONNECTION_DELETE_FAILED);
        self.repo
            .get(id)
            .await
            .map_err(wrap)?
            .ok_or_else(not_found)?;
        self.repo.delete(id, self.now()).await.map_err(wrap)
    }

    pub async fn set_status(&self, id: Id, status: ConnectionStatus) -> Result<()> {
        let wrap = |e: Error| e.or_internal(keys::CONNECTION_UPDATE_FAILED);
        let mut conn = self
            .repo
            .get(id)
            .await
            .map_err(wrap)?
            .ok_or_else(not_found)?;
        conn.status = status;
        self.repo.save(&conn, self.now()).await.map_err(wrap)
    }

    /// Endpoint with the decrypted secret and the negotiated state; a decrypt failure
    /// is an error, never a fallback to the ciphertext (UPS-02).
    pub fn endpoint(&self, conn: &SiteConnection) -> Result<Endpoint> {
        let secret = self.decrypt_secret(conn)?;
        Ok(Endpoint {
            connection_id: conn.id,
            base_url: conn.base_url.clone(),
            api_key: conn.api_key.clone(),
            api_secret: secret,
            protocol: conn.protocol.clone(),
            features: conn.state.negotiated.then(|| conn.state.features.clone()),
            extra: conn.state.extra.clone(),
        })
    }

    pub fn decrypt_secret(&self, conn: &SiteConnection) -> Result<String> {
        self.cipher
            .decrypt(&conn.api_secret)
            .map_err(Error::internal)
    }

    /// Outbound client of a connection (errors early on a bad secret / protocol, UPS-06).
    pub fn client(&self, conn: &SiteConnection) -> Result<Arc<dyn UpstreamClient>> {
        self.connector.open(&self.endpoint(conn)?)
    }

    /// Client of a connection id (`error.connection_not_found` when missing).
    pub async fn client_of(&self, id: Id) -> Result<(SiteConnection, Arc<dyn UpstreamClient>)> {
        let conn = self.repo.get(id).await?.ok_or_else(not_found)?;
        let client = self.client(&conn)?;
        Ok((conn, client))
    }

    /// Tests the connection with the adapter handshake (negotiated state refreshed);
    /// the outcome is stored either way and a pending connection becomes active on
    /// success. Supplier errors are returned verbatim.
    pub async fn ping(&self, id: Id) -> Result<PingInfo> {
        let mut conn = self.repo.get(id).await?.ok_or_else(not_found)?;
        let result = self.negotiate(&mut conn).await;
        conn.last_ping_at = Some(self.now());
        conn.last_ping_ok = result.is_ok();
        if result.is_ok() && conn.status == ConnectionStatus::Pending {
            conn.status = ConnectionStatus::Active;
        }
        if let Err(error) = self.repo.save(&conn, self.now()).await {
            tracing::warn!(%error, connection_id = id, "store ping result failed");
        }
        let info = result?;
        Ok(PingInfo {
            site_name: info.site_name,
            protocol_version: info.version,
            user_id: info.user_id,
            balance: info.balance,
            currency: info.account_currency,
            member_level: info.member_level,
        })
    }

    /// Re-applies the connection pricing to every mapped product; returns the count.
    pub async fn reapply_markup(&self, id: Id) -> Result<u64> {
        let conn = self
            .repo
            .get(id)
            .await
            .map_err(|e| e.or_internal(keys::REAPPLY_MARKUP_FAILED))?
            .ok_or_else(not_found)?;
        self.mappings
            .reapply_pricing(conn.id, &conn.pricing(), self.now())
            .await
            .map_err(|e| e.or_internal(keys::REAPPLY_MARKUP_FAILED))
    }
}
