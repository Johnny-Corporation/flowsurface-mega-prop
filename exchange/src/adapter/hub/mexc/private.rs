use crate::{adapter::AdapterError, proxy::Proxy};
use hmac::{Hmac, Mac};
use reqwest::{Method, header};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::Sha256;
use std::{
    collections::BTreeMap,
    fmt,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use url::form_urlencoded;

use super::FETCH_DOMAIN;

const HTTP_CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const HTTP_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);
const DEFAULT_RECV_WINDOW_MS: u64 = 5_000;
const DEFAULT_FUTURES_RECV_WINDOW_MS: u64 = 5_000;

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, PartialEq, Eq)]
pub struct MexcCredentials {
    access_key: String,
    secret_key: String,
}

impl MexcCredentials {
    pub fn new(
        access_key: impl Into<String>,
        secret_key: impl Into<String>,
    ) -> Result<Self, String> {
        let access_key = access_key.into().trim().to_string();
        let secret_key = secret_key.into().trim().to_string();

        validate_credential_field("access key", &access_key)?;
        validate_credential_field("secret key", &secret_key)?;

        Ok(Self {
            access_key,
            secret_key,
        })
    }

    pub fn access_key(&self) -> &str {
        &self.access_key
    }
}

impl fmt::Debug for MexcCredentials {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MexcCredentials")
            .field("access_key", &masked_value(&self.access_key))
            .field("secret_key", &"<redacted>")
            .finish()
    }
}

#[derive(Clone)]
pub struct MexcPrivateClient {
    client: reqwest::Client,
    credentials: MexcCredentials,
    spot_recv_window_ms: u64,
    futures_recv_window_ms: u64,
}

#[derive(Clone)]
pub struct MexcBlockingPrivateClient {
    client: reqwest::blocking::Client,
    credentials: MexcCredentials,
    spot_recv_window_ms: u64,
    futures_recv_window_ms: u64,
}

impl MexcPrivateClient {
    pub fn new(credentials: MexcCredentials, proxy: Option<&Proxy>) -> Result<Self, AdapterError> {
        let builder = reqwest::Client::builder()
            .connect_timeout(HTTP_CONNECT_TIMEOUT)
            .timeout(HTTP_REQUEST_TIMEOUT);
        let builder = crate::adapter::proxy::try_apply_proxy(builder, proxy);
        let client = builder.build().map_err(|error| {
            AdapterError::InvalidRequest(format!(
                "Failed to build MEXC private HTTP client: {error}"
            ))
        })?;

        Ok(Self {
            client,
            credentials,
            spot_recv_window_ms: DEFAULT_RECV_WINDOW_MS,
            futures_recv_window_ms: DEFAULT_FUTURES_RECV_WINDOW_MS,
        })
    }

    pub async fn spot_account_information(&self) -> Result<Value, AdapterError> {
        self.send_spot_signed(Method::GET, "/v3/account", Vec::new())
            .await
    }

    pub async fn spot_test_order(&self, request: &SpotOrderRequest) -> Result<Value, AdapterError> {
        self.send_spot_signed(Method::POST, "/v3/order/test", request.params())
            .await
    }

    pub async fn spot_place_order(
        &self,
        request: &SpotOrderRequest,
    ) -> Result<Value, AdapterError> {
        self.send_spot_signed(Method::POST, "/v3/order", request.params())
            .await
    }

    pub async fn futures_assets(&self) -> Result<MexcFuturesResponse<Value>, AdapterError> {
        self.send_futures_signed(Method::GET, "/v1/private/account/assets", Vec::new(), None)
            .await
    }

    pub async fn futures_open_positions(
        &self,
        symbol: Option<&str>,
    ) -> Result<MexcFuturesResponse<Value>, AdapterError> {
        let params = symbol
            .map(|symbol| vec![("symbol".to_string(), symbol.to_string())])
            .unwrap_or_default();

        self.send_futures_signed(
            Method::GET,
            "/v1/private/position/open_positions",
            params,
            None,
        )
        .await
    }

    pub async fn futures_open_orders(
        &self,
        page_num: u32,
        page_size: u32,
    ) -> Result<MexcFuturesResponse<Value>, AdapterError> {
        let params = vec![
            ("page_num".to_string(), page_num.max(1).to_string()),
            ("page_size".to_string(), page_size.clamp(1, 100).to_string()),
        ];

        self.send_futures_signed(
            Method::GET,
            "/v1/private/order/list/open_orders",
            params,
            None,
        )
        .await
    }

    pub async fn futures_history_orders(
        &self,
        page_num: u32,
        page_size: u32,
    ) -> Result<MexcFuturesResponse<Value>, AdapterError> {
        let params = vec![
            ("page_num".to_string(), page_num.max(1).to_string()),
            ("page_size".to_string(), page_size.clamp(1, 100).to_string()),
        ];

        self.send_futures_signed(
            Method::GET,
            "/v1/private/order/list/history_orders",
            params,
            None,
        )
        .await
    }

    pub async fn futures_place_order(
        &self,
        request: &FuturesOrderRequest,
    ) -> Result<MexcFuturesResponse<Value>, AdapterError> {
        self.send_futures_signed(
            Method::POST,
            "/v1/private/order/create",
            Vec::new(),
            Some(request.body()),
        )
        .await
    }

    async fn send_spot_signed(
        &self,
        method: Method,
        path: &str,
        mut params: Vec<(&'static str, String)>,
    ) -> Result<Value, AdapterError> {
        params.push(("recvWindow", self.spot_recv_window_ms.to_string()));
        params.push(("timestamp", now_ms().to_string()));

        let query = encode_pairs(params);
        let signature = sign_spot_payload(&query, &self.credentials.secret_key);
        let url = format!("{FETCH_DOMAIN}{path}?{query}&signature={signature}");

        let response = self
            .client
            .request(method.clone(), &url)
            .header("X-MEXC-APIKEY", self.credentials.access_key())
            .send()
            .await
            .map_err(|error| {
                AdapterError::InvalidRequest(format!(
                    "MEXC spot private request failed for {path}: {error}"
                ))
            })?;

        parse_json_response(method, path, response).await
    }

    async fn send_futures_signed(
        &self,
        method: Method,
        path: &str,
        params: Vec<(String, String)>,
        body: Option<Value>,
    ) -> Result<MexcFuturesResponse<Value>, AdapterError> {
        let timestamp = now_ms();
        let query = encode_sorted_params(params);
        let body_string = match body {
            Some(value) => serde_json::to_string(&value).map_err(|error| {
                AdapterError::ParseError(format!("Failed to serialize MEXC futures body: {error}"))
            })?,
            None => String::new(),
        };
        let signing_params = if method == Method::POST {
            body_string.as_str()
        } else {
            query.as_str()
        };
        let signature = sign_futures_payload(
            self.credentials.access_key(),
            timestamp,
            signing_params,
            &self.credentials.secret_key,
        );
        let url = if query.is_empty() {
            format!("{FETCH_DOMAIN}{path}")
        } else {
            format!("{FETCH_DOMAIN}{path}?{query}")
        };

        let mut builder = self
            .client
            .request(method.clone(), &url)
            .header("ApiKey", self.credentials.access_key())
            .header("Request-Time", timestamp.to_string())
            .header("Signature", signature)
            .header("Recv-Window", self.futures_recv_window_ms.to_string());

        if method == Method::POST {
            builder = builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(body_string);
        }

        let response = builder.send().await.map_err(|error| {
            AdapterError::InvalidRequest(format!(
                "MEXC futures private request failed for {path}: {error}"
            ))
        })?;

        parse_json_response(method, path, response).await
    }
}

impl MexcBlockingPrivateClient {
    pub fn new(credentials: MexcCredentials, proxy: Option<&Proxy>) -> Result<Self, AdapterError> {
        let builder = reqwest::blocking::Client::builder()
            .connect_timeout(HTTP_CONNECT_TIMEOUT)
            .timeout(HTTP_REQUEST_TIMEOUT);
        let builder = crate::adapter::proxy::try_apply_blocking_proxy(builder, proxy);
        let client = builder.build().map_err(|error| {
            AdapterError::InvalidRequest(format!(
                "Failed to build blocking MEXC private HTTP client: {error}"
            ))
        })?;

        Ok(Self {
            client,
            credentials,
            spot_recv_window_ms: DEFAULT_RECV_WINDOW_MS,
            futures_recv_window_ms: DEFAULT_FUTURES_RECV_WINDOW_MS,
        })
    }

    pub fn spot_account_information(&self) -> Result<Value, AdapterError> {
        self.send_spot_signed(Method::GET, "/v3/account", Vec::new())
    }

    pub fn futures_assets(&self) -> Result<MexcFuturesResponse<Value>, AdapterError> {
        self.send_futures_signed(Method::GET, "/v1/private/account/assets", Vec::new(), None)
    }

    pub fn futures_history_orders(
        &self,
        page_num: u32,
        page_size: u32,
    ) -> Result<MexcFuturesResponse<Value>, AdapterError> {
        let params = vec![
            ("page_num".to_string(), page_num.max(1).to_string()),
            ("page_size".to_string(), page_size.clamp(1, 100).to_string()),
        ];

        self.send_futures_signed(
            Method::GET,
            "/v1/private/order/list/history_orders",
            params,
            None,
        )
    }

    fn send_spot_signed(
        &self,
        method: Method,
        path: &str,
        mut params: Vec<(&'static str, String)>,
    ) -> Result<Value, AdapterError> {
        params.push(("recvWindow", self.spot_recv_window_ms.to_string()));
        params.push(("timestamp", now_ms().to_string()));

        let query = encode_pairs(params);
        let signature = sign_spot_payload(&query, &self.credentials.secret_key);
        let url = format!("{FETCH_DOMAIN}{path}?{query}&signature={signature}");

        let response = self
            .client
            .request(method.clone(), &url)
            .header("X-MEXC-APIKEY", self.credentials.access_key())
            .send()
            .map_err(|error| {
                AdapterError::InvalidRequest(format!(
                    "MEXC spot private request failed for {path}: {error}"
                ))
            })?;

        parse_blocking_json_response(method, path, response)
    }

    fn send_futures_signed(
        &self,
        method: Method,
        path: &str,
        params: Vec<(String, String)>,
        body: Option<Value>,
    ) -> Result<MexcFuturesResponse<Value>, AdapterError> {
        let timestamp = now_ms();
        let query = encode_sorted_params(params);
        let body_string = match body {
            Some(value) => serde_json::to_string(&value).map_err(|error| {
                AdapterError::ParseError(format!("Failed to serialize MEXC futures body: {error}"))
            })?,
            None => String::new(),
        };
        let signing_params = if method == Method::POST {
            body_string.as_str()
        } else {
            query.as_str()
        };
        let signature = sign_futures_payload(
            self.credentials.access_key(),
            timestamp,
            signing_params,
            &self.credentials.secret_key,
        );
        let url = if query.is_empty() {
            format!("{FETCH_DOMAIN}{path}")
        } else {
            format!("{FETCH_DOMAIN}{path}?{query}")
        };

        let mut builder = self
            .client
            .request(method.clone(), &url)
            .header("ApiKey", self.credentials.access_key())
            .header("Request-Time", timestamp.to_string())
            .header("Signature", signature)
            .header("Recv-Window", self.futures_recv_window_ms.to_string());

        if method == Method::POST {
            builder = builder
                .header(header::CONTENT_TYPE, "application/json")
                .body(body_string);
        }

        let response = builder.send().map_err(|error| {
            AdapterError::InvalidRequest(format!(
                "MEXC futures private request failed for {path}: {error}"
            ))
        })?;

        parse_blocking_json_response(method, path, response)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpotOrderRequest {
    symbol: String,
    side: SpotOrderSide,
    order_type: SpotOrderType,
    quantity: Option<String>,
    quote_order_qty: Option<String>,
    price: Option<String>,
    new_client_order_id: Option<String>,
}

impl SpotOrderRequest {
    pub fn limit(
        symbol: impl Into<String>,
        side: SpotOrderSide,
        quantity: impl ToString,
        price: impl ToString,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            side,
            order_type: SpotOrderType::Limit,
            quantity: Some(quantity.to_string()),
            quote_order_qty: None,
            price: Some(price.to_string()),
            new_client_order_id: None,
        }
    }

    pub fn market_quantity(
        symbol: impl Into<String>,
        side: SpotOrderSide,
        quantity: impl ToString,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            side,
            order_type: SpotOrderType::Market,
            quantity: Some(quantity.to_string()),
            quote_order_qty: None,
            price: None,
            new_client_order_id: None,
        }
    }

    pub fn market_quote_quantity(
        symbol: impl Into<String>,
        side: SpotOrderSide,
        quote_order_qty: impl ToString,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            side,
            order_type: SpotOrderType::Market,
            quantity: None,
            quote_order_qty: Some(quote_order_qty.to_string()),
            price: None,
            new_client_order_id: None,
        }
    }

    pub fn with_client_order_id(mut self, new_client_order_id: impl Into<String>) -> Self {
        self.new_client_order_id = Some(new_client_order_id.into());
        self
    }

    fn params(&self) -> Vec<(&'static str, String)> {
        let mut params = vec![
            ("symbol", self.symbol.clone()),
            ("side", self.side.as_str().to_string()),
            ("type", self.order_type.as_str().to_string()),
        ];

        if let Some(quantity) = &self.quantity {
            params.push(("quantity", quantity.clone()));
        }
        if let Some(quote_order_qty) = &self.quote_order_qty {
            params.push(("quoteOrderQty", quote_order_qty.clone()));
        }
        if let Some(price) = &self.price {
            params.push(("price", price.clone()));
        }
        if let Some(new_client_order_id) = &self.new_client_order_id {
            params.push(("newClientOrderId", new_client_order_id.clone()));
        }

        params
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpotOrderSide {
    Buy,
    Sell,
}

impl SpotOrderSide {
    fn as_str(self) -> &'static str {
        match self {
            Self::Buy => "BUY",
            Self::Sell => "SELL",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpotOrderType {
    Limit,
    Market,
}

impl SpotOrderType {
    fn as_str(self) -> &'static str {
        match self {
            Self::Limit => "LIMIT",
            Self::Market => "MARKET",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FuturesOrderRequest {
    symbol: String,
    price: String,
    vol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    leverage: Option<u32>,
    side: i32,
    #[serde(rename = "type")]
    order_type: i32,
    open_type: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    external_oid: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reduce_only: Option<bool>,
}

impl FuturesOrderRequest {
    pub fn new(
        symbol: impl Into<String>,
        price: impl ToString,
        vol: impl ToString,
        side: FuturesOrderSide,
        order_type: FuturesOrderType,
        open_type: FuturesOpenType,
    ) -> Self {
        Self {
            symbol: symbol.into(),
            price: price.to_string(),
            vol: vol.to_string(),
            leverage: None,
            side: side as i32,
            order_type: order_type as i32,
            open_type: open_type as i32,
            external_oid: None,
            reduce_only: None,
        }
    }

    pub fn with_leverage(mut self, leverage: u32) -> Self {
        self.leverage = Some(leverage.max(1));
        self
    }

    pub fn with_external_oid(mut self, external_oid: impl Into<String>) -> Self {
        self.external_oid = Some(external_oid.into());
        self
    }

    pub fn with_reduce_only(mut self, reduce_only: bool) -> Self {
        self.reduce_only = Some(reduce_only);
        self
    }

    fn body(&self) -> Value {
        // JSON field names must match MEXC futures exactly; camelCase is doing the quiet heavy lift here.
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuturesOrderSide {
    OpenLong = 1,
    CloseShort = 2,
    OpenShort = 3,
    CloseLong = 4,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuturesOrderType {
    Limit = 1,
    PostOnly = 2,
    Ioc = 3,
    Fok = 4,
    Market = 5,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FuturesOpenType {
    Isolated = 1,
    Cross = 2,
}

#[derive(Debug, Clone, Deserialize)]
pub struct MexcFuturesResponse<T = Value> {
    pub success: bool,
    pub code: i64,
    #[serde(default)]
    pub message: Option<String>,
    #[serde(default)]
    pub data: Option<T>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MexcAvailableBalance {
    pub asset: String,
    pub available: String,
}

pub fn available_balances_from_spot_account(account: &Value) -> Vec<MexcAvailableBalance> {
    account
        .get("balances")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|balance| {
            let asset = balance.get("asset").and_then(Value::as_str)?;
            let available = value_as_display_string(balance.get("free")?)?;
            Some(MexcAvailableBalance {
                asset: asset.to_string(),
                available,
            })
        })
        .collect()
}

pub fn available_balances_from_futures_assets(
    response: &MexcFuturesResponse<Value>,
) -> Vec<MexcAvailableBalance> {
    let Some(data) = &response.data else {
        return Vec::new();
    };

    let mut balances = Vec::new();
    collect_futures_balances(data, &mut balances);
    balances
}

pub(super) fn sign_spot_payload(payload: &str, secret_key: &str) -> String {
    hmac_sha256_hex(payload, secret_key)
}

pub(super) fn sign_futures_payload(
    access_key: &str,
    timestamp_ms: u64,
    params: &str,
    secret_key: &str,
) -> String {
    let payload = format!("{access_key}{timestamp_ms}{params}");
    hmac_sha256_hex(&payload, secret_key)
}

fn hmac_sha256_hex(payload: &str, secret_key: &str) -> String {
    let mut mac =
        HmacSha256::new_from_slice(secret_key.as_bytes()).expect("HMAC accepts any key length");
    mac.update(payload.as_bytes());
    lower_hex(&mac.finalize().into_bytes())
}

fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";
    let mut output = String::with_capacity(bytes.len() * 2);

    for byte in bytes {
        output.push(HEX[(byte >> 4) as usize] as char);
        output.push(HEX[(byte & 0x0f) as usize] as char);
    }

    output
}

fn encode_pairs(params: Vec<(&'static str, String)>) -> String {
    let mut serializer = form_urlencoded::Serializer::new(String::new());

    for (key, value) in params {
        serializer.append_pair(key, &value);
    }

    serializer.finish()
}

fn encode_sorted_params(params: Vec<(String, String)>) -> String {
    let sorted = params.into_iter().collect::<BTreeMap<_, _>>();
    let mut serializer = form_urlencoded::Serializer::new(String::new());

    for (key, value) in sorted {
        serializer.append_pair(&key, &value);
    }

    serializer.finish()
}

fn collect_futures_balances(value: &Value, balances: &mut Vec<MexcAvailableBalance>) {
    match value {
        Value::Array(items) => {
            for item in items {
                collect_futures_balances(item, balances);
            }
        }
        Value::Object(object) => {
            let asset = object
                .get("currency")
                .or_else(|| object.get("asset"))
                .or_else(|| object.get("coin"))
                .and_then(Value::as_str);
            let available = object
                .get("availableBalance")
                .or_else(|| object.get("available"))
                .or_else(|| object.get("availableMargin"))
                .and_then(value_as_display_string);

            if let (Some(asset), Some(available)) = (asset, available) {
                balances.push(MexcAvailableBalance {
                    asset: asset.to_string(),
                    available,
                });
            }

            for nested in object.values() {
                if matches!(nested, Value::Array(_) | Value::Object(_)) {
                    collect_futures_balances(nested, balances);
                }
            }
        }
        _ => {}
    }
}

fn value_as_display_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

async fn parse_json_response<T>(
    method: Method,
    path: &str,
    response: reqwest::Response,
) -> Result<T, AdapterError>
where
    T: for<'de> Deserialize<'de>,
{
    let status = response.status();
    let body = response.text().await.map_err(|error| {
        AdapterError::InvalidRequest(format!(
            "Failed to read MEXC private response for {method} {path}: {error}"
        ))
    })?;

    if !status.is_success() {
        return Err(AdapterError::http_status_failed(
            status,
            format!(
                "MEXC private request {method} {path} returned HTTP {status}: {}",
                body_preview(&body, 200)
            ),
        ));
    }

    serde_json::from_str(&body).map_err(|error| {
        AdapterError::ParseError(format!(
            "Failed to parse MEXC private response for {method} {path}: {error}; preview={}",
            body_preview(&body, 200)
        ))
    })
}

fn parse_blocking_json_response<T>(
    method: Method,
    path: &str,
    response: reqwest::blocking::Response,
) -> Result<T, AdapterError>
where
    T: for<'de> Deserialize<'de>,
{
    let status = response.status();
    let body = response.text().map_err(|error| {
        AdapterError::InvalidRequest(format!(
            "Failed to read MEXC private response for {method} {path}: {error}"
        ))
    })?;

    if !status.is_success() {
        return Err(AdapterError::http_status_failed(
            status,
            format!(
                "MEXC private request {method} {path} returned HTTP {status}: {}",
                body_preview(&body, 200)
            ),
        ));
    }

    serde_json::from_str(&body).map_err(|error| {
        AdapterError::ParseError(format!(
            "Failed to parse MEXC private response for {method} {path}: {error}; preview={}",
            body_preview(&body, 200)
        ))
    })
}

fn body_preview(body: &str, limit: usize) -> String {
    let trimmed = body.trim();
    let mut preview = trimmed.chars().take(limit).collect::<String>();

    if trimmed.chars().count() > limit {
        preview.push_str("...");
    }

    preview
}

fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis().min(u64::MAX as u128) as u64)
        .unwrap_or_default()
}

fn validate_credential_field(label: &str, value: &str) -> Result<(), String> {
    if value.is_empty() {
        return Err(format!("MEXC {label} cannot be empty"));
    }

    if value.contains('\n') || value.contains('\r') {
        return Err(format!("MEXC {label} cannot contain line breaks"));
    }

    Ok(())
}

fn masked_value(value: &str) -> String {
    let len = value.chars().count();
    if len <= 4 {
        return "****".to_string();
    }

    let suffix = value
        .chars()
        .skip(len.saturating_sub(4))
        .collect::<String>();
    format!("****{suffix}")
}

#[cfg(test)]
mod tests {
    use super::{
        MexcFuturesResponse, available_balances_from_futures_assets,
        available_balances_from_spot_account, sign_futures_payload, sign_spot_payload,
    };

    #[test]
    fn spot_signature_matches_mexc_example() {
        let payload = "symbol=BTCUSDT&side=BUY&type=LIMIT&quantity=1&price=11&recvWindow=5000&timestamp=1644489390087";
        let signature = sign_spot_payload(payload, "45d0b3c26f2644f19bfb98b07741b2f5");

        assert_eq!(
            signature,
            "fd3e4e8543c5188531eb7279d68ae7d26a573d0fc5ab0d18eb692451654d837a"
        );
    }

    #[test]
    fn futures_signature_uses_access_key_timestamp_and_params() {
        let signature = sign_futures_payload(
            "mx-access-key",
            1_710_000_000_000,
            "symbol=BTC_USDT",
            "mx-secret-key",
        );

        assert_eq!(
            signature,
            "4001b32232a745d72225106b7ff1a2c82ae8334089faf3e4291a0248748769e8"
        );
    }

    #[test]
    fn futures_recv_window_uses_milliseconds() {
        assert_eq!(super::DEFAULT_FUTURES_RECV_WINDOW_MS, 5_000);
    }

    #[test]
    fn spot_available_balances_extracts_free_balances() {
        let response = serde_json::json!({
            "balances": [
                {"asset": "USDT", "free": "123.45000000", "locked": "0"},
                {"asset": "BTC", "free": "0.01000000", "locked": "0.002"}
            ]
        });

        let balances = available_balances_from_spot_account(&response);

        assert_eq!(balances[0].asset, "USDT");
        assert_eq!(balances[0].available, "123.45000000");
        assert_eq!(balances[1].asset, "BTC");
        assert_eq!(balances[1].available, "0.01000000");
    }

    #[test]
    fn futures_available_balances_extracts_asset_available_balance() {
        let response = MexcFuturesResponse {
            success: true,
            code: 0,
            message: None,
            data: Some(serde_json::json!([
                {"currency": "USDT", "availableBalance": "99.5"},
                {"currency": "BTC", "availableBalance": 0.25}
            ])),
        };

        let balances = available_balances_from_futures_assets(&response);

        assert_eq!(balances[0].asset, "USDT");
        assert_eq!(balances[0].available, "99.5");
        assert_eq!(balances[1].asset, "BTC");
        assert_eq!(balances[1].available, "0.25");
    }
}
