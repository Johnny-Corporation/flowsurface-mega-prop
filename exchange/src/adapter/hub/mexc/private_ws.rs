use super::{
    MEXC_FUTURES_WS_DOMAIN, MEXC_FUTURES_WS_PATH, PING_INTERVAL, private::MexcCredentials,
};
use crate::{
    adapter::{
        AdapterError,
        connect::{State, connect_ws},
    },
    proxy::Proxy,
};
use fastwebsockets::{FragmentCollector, Frame, OpCode};
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use serde_json::{Value, json};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

#[derive(Debug, Clone)]
pub enum MexcPrivateWsEvent {
    Connected,
    LoggedIn,
    Disconnected(String),
    Error(String),
    Order(MexcPrivateOrderUpdate),
    Position(MexcPrivatePositionUpdate),
}

#[derive(Debug, Clone, PartialEq)]
pub struct MexcPrivateOrderUpdate {
    pub symbol: String,
    pub side: i64,
    pub state: i64,
    pub price: f32,
    pub vol: f32,
    pub remain_vol: Option<f32>,
    pub order_id: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct MexcPrivatePositionUpdate {
    pub symbol: String,
    pub position_type: i64,
    pub state: Option<i64>,
    pub hold_vol: f32,
    pub hold_avg_price: Option<f32>,
    pub open_avg_price: Option<f32>,
    pub realised: f32,
}

pub fn run_mexc_private_ws_blocking<F>(
    credentials: MexcCredentials,
    proxy_cfg: Option<Proxy>,
    stop: Arc<AtomicBool>,
    emit: F,
) where
    F: FnMut(MexcPrivateWsEvent) + Send + 'static,
{
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(runtime) => runtime,
        Err(error) => {
            let mut emit = emit;
            emit(MexcPrivateWsEvent::Error(format!(
                "Failed to start MEXC private WebSocket runtime: {error}"
            )));
            return;
        }
    };

    runtime.block_on(run_mexc_private_ws(credentials, proxy_cfg, stop, emit));
}

async fn run_mexc_private_ws<F>(
    credentials: MexcCredentials,
    proxy_cfg: Option<Proxy>,
    stop: Arc<AtomicBool>,
    mut emit: F,
) where
    F: FnMut(MexcPrivateWsEvent),
{
    let mut state = State::Disconnected;
    let mut ping_interval = tokio::time::interval(Duration::from_secs(PING_INTERVAL));
    let mut stop_interval = tokio::time::interval(Duration::from_millis(250));

    while !stop.load(Ordering::Relaxed) {
        match &mut state {
            State::Disconnected => match connect_websocket(proxy_cfg.as_ref()).await {
                Ok(mut websocket) => {
                    emit(MexcPrivateWsEvent::Connected);
                    if let Err(error) = login(&mut websocket, &credentials).await {
                        emit(MexcPrivateWsEvent::Disconnected(error.to_string()));
                        tokio::time::sleep(Duration::from_secs(1)).await;
                        continue;
                    }
                    state = State::Connected(websocket);
                }
                Err(error) => {
                    emit(MexcPrivateWsEvent::Disconnected(error.to_string()));
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            },
            State::Connected(websocket) => {
                tokio::select! {
                    _ = stop_interval.tick() => {
                        if stop.load(Ordering::Relaxed) {
                            break;
                        }
                    }
                    _ = ping_interval.tick() => {
                        if let Err(error) = send_json(websocket, &json!({"method": "ping"})).await {
                            emit(MexcPrivateWsEvent::Disconnected(error.to_string()));
                            state = State::Disconnected;
                        }
                    }
                    frame = websocket.read_frame() => {
                        match frame {
                            Ok(frame) => match frame.opcode {
                                OpCode::Text => {
                                    match parse_private_event(&frame.payload) {
                                        Ok(Some(MexcPrivateWsEvent::LoggedIn)) => {
                                            emit(MexcPrivateWsEvent::LoggedIn);
                                            if let Err(error) = subscribe_private_updates(websocket).await {
                                                emit(MexcPrivateWsEvent::Error(error.to_string()));
                                            }
                                        }
                                        Ok(Some(event)) => emit(event),
                                        Ok(None) => {}
                                        Err(error) => {
                                            emit(MexcPrivateWsEvent::Error(error.to_string()));
                                        }
                                    }
                                }
                                OpCode::Close => {
                                    emit(MexcPrivateWsEvent::Disconnected(
                                        "MEXC private WebSocket closed".to_string(),
                                    ));
                                    state = State::Disconnected;
                                }
                                _ => {}
                            },
                            Err(error) => {
                                emit(MexcPrivateWsEvent::Disconnected(format!(
                                    "MEXC private WebSocket read failed: {error}"
                                )));
                                state = State::Disconnected;
                            }
                        }
                    }
                }
            }
        }
    }
}

async fn connect_websocket(
    proxy_cfg: Option<&Proxy>,
) -> Result<FragmentCollector<TokioIo<Upgraded>>, AdapterError> {
    let url = format!("wss://{}{}", MEXC_FUTURES_WS_DOMAIN, MEXC_FUTURES_WS_PATH);
    connect_ws(MEXC_FUTURES_WS_DOMAIN, &url, proxy_cfg).await
}

async fn login(
    websocket: &mut FragmentCollector<TokioIo<Upgraded>>,
    credentials: &MexcCredentials,
) -> Result<(), AdapterError> {
    let req_time = now_ms();
    let payload = json!({
        "method": "login",
        "param": {
            "apiKey": credentials.access_key(),
            "reqTime": req_time.to_string(),
            "signature": credentials.futures_ws_signature(req_time),
        }
    });

    send_json(websocket, &payload).await
}

async fn subscribe_private_updates(
    websocket: &mut FragmentCollector<TokioIo<Upgraded>>,
) -> Result<(), AdapterError> {
    let payload = json!({
        "method": "personal.filter",
        "param": {
            "filters": [
                { "filter": "order" },
                { "filter": "order.deal" },
                { "filter": "position" },
                { "filter": "asset" }
            ]
        }
    });

    send_json(websocket, &payload).await
}

async fn send_json(
    websocket: &mut FragmentCollector<TokioIo<Upgraded>>,
    payload: &Value,
) -> Result<(), AdapterError> {
    let payload = payload.to_string();
    websocket
        .write_frame(Frame::text(fastwebsockets::Payload::Borrowed(
            payload.as_bytes(),
        )))
        .await
        .map_err(|error| {
            AdapterError::WebsocketError(format!(
                "Failed to write MEXC private WebSocket frame: {error}"
            ))
        })
}

fn parse_private_event(payload: &[u8]) -> Result<Option<MexcPrivateWsEvent>, AdapterError> {
    let value: Value = serde_json::from_slice(payload)
        .map_err(|error| AdapterError::ParseError(error.to_string()))?;
    let channel = value
        .get("channel")
        .and_then(Value::as_str)
        .unwrap_or_default();

    match channel {
        "pong" => Ok(None),
        "rs.login" => Ok(Some(MexcPrivateWsEvent::LoggedIn)),
        "rs.error" => Ok(Some(MexcPrivateWsEvent::Error(display_data(&value)))),
        "push.personal.order" => {
            let data = value.get("data").ok_or_else(|| {
                AdapterError::ParseError("MEXC private order update missing data".to_string())
            })?;
            Ok(Some(MexcPrivateWsEvent::Order(parse_order_update(data)?)))
        }
        "push.personal.position" => {
            let data = value.get("data").ok_or_else(|| {
                AdapterError::ParseError("MEXC private position update missing data".to_string())
            })?;
            Ok(Some(MexcPrivateWsEvent::Position(parse_position_update(
                data,
            )?)))
        }
        _ => Ok(None),
    }
}

fn parse_order_update(data: &Value) -> Result<MexcPrivateOrderUpdate, AdapterError> {
    let symbol = string_field(data, "symbol")?;
    let side = i64_field(data, "side")?;
    let state = i64_field(data, "state")?;
    let price = f32_field(data, "price")?;
    let vol = f32_field(data, "vol")?;
    let remain_vol = data.get("remainVol").and_then(value_as_f32);
    let order_id = string_field(data, "orderId")
        .or_else(|_| string_field(data, "externalOid"))
        .unwrap_or_else(|_| format!("{symbol}:{side}:{price}:{vol}"));

    Ok(MexcPrivateOrderUpdate {
        symbol,
        side,
        state,
        price,
        vol,
        remain_vol,
        order_id,
    })
}

fn parse_position_update(data: &Value) -> Result<MexcPrivatePositionUpdate, AdapterError> {
    Ok(MexcPrivatePositionUpdate {
        symbol: string_field(data, "symbol")?,
        position_type: i64_field(data, "positionType")?,
        state: data.get("state").and_then(value_as_i64),
        hold_vol: data
            .get("holdVol")
            .and_then(value_as_f32)
            .unwrap_or_default(),
        hold_avg_price: data.get("holdAvgPrice").and_then(value_as_f32),
        open_avg_price: data.get("openAvgPrice").and_then(value_as_f32),
        realised: data
            .get("realised")
            .and_then(value_as_f32)
            .unwrap_or_default(),
    })
}

fn display_data(value: &Value) -> String {
    value
        .get("data")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| value.to_string())
}

fn string_field(value: &Value, key: &str) -> Result<String, AdapterError> {
    value
        .get(key)
        .and_then(value_as_display_string)
        .ok_or_else(|| AdapterError::ParseError(format!("MEXC private field missing: {key}")))
}

fn i64_field(value: &Value, key: &str) -> Result<i64, AdapterError> {
    value
        .get(key)
        .and_then(value_as_i64)
        .ok_or_else(|| AdapterError::ParseError(format!("MEXC private field missing: {key}")))
}

fn f32_field(value: &Value, key: &str) -> Result<f32, AdapterError> {
    value
        .get(key)
        .and_then(value_as_f32)
        .ok_or_else(|| AdapterError::ParseError(format!("MEXC private field missing: {key}")))
}

fn value_as_display_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

fn value_as_i64(value: &Value) -> Option<i64> {
    value
        .as_i64()
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn value_as_f32(value: &Value) -> Option<f32> {
    value
        .as_f64()
        .map(|value| value as f32)
        .or_else(|| value.as_str().and_then(|value| value.parse().ok()))
}

fn now_ms() -> u64 {
    chrono::Utc::now().timestamp_millis().max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::{MexcPrivateWsEvent, parse_private_event};

    #[test]
    fn parses_private_order_push() {
        let payload = br#"{
            "channel": "push.personal.order",
            "data": {
                "orderId": "102067003631907840",
                "symbol": "CRV_USDT",
                "side": 4,
                "state": 2,
                "price": 0.707,
                "vol": 3,
                "remainVol": 2
            },
            "ts": 1610005069989
        }"#;

        let event = parse_private_event(payload).unwrap().unwrap();
        let MexcPrivateWsEvent::Order(order) = event else {
            panic!("expected order event");
        };

        assert_eq!(order.symbol, "CRV_USDT");
        assert_eq!(order.side, 4);
        assert_eq!(order.state, 2);
        assert_eq!(order.order_id, "102067003631907840");
        assert_eq!(order.remain_vol, Some(2.0));
    }

    #[test]
    fn parses_private_position_push() {
        let payload = br#"{
            "channel": "push.personal.position",
            "data": {
                "symbol": "BTC_USDT",
                "positionType": 1,
                "state": 1,
                "holdVol": "2",
                "holdAvgPrice": "64000.5",
                "realised": "-0.25"
            },
            "ts": 1610005070157
        }"#;

        let event = parse_private_event(payload).unwrap().unwrap();
        let MexcPrivateWsEvent::Position(position) = event else {
            panic!("expected position event");
        };

        assert_eq!(position.symbol, "BTC_USDT");
        assert_eq!(position.position_type, 1);
        assert_eq!(position.hold_vol, 2.0);
        assert_eq!(position.hold_avg_price, Some(64000.5));
        assert_eq!(position.realised, -0.25);
    }
}
