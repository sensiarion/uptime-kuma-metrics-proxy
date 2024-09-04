use crate::common::config::KumaConnectionConfig;
use futures_util::FutureExt;
use rust_socketio::{
    asynchronous::{Client, ClientBuilder},
    Payload, TransportType,
};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use tokio::sync::broadcast::channel;
use tokio::time;

pub enum ServiceReceiveError {
    Conn(rust_socketio::Error),
    Logic(String),
}

impl fmt::Debug for ServiceReceiveError {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            ServiceReceiveError::Conn(err) => {
                write!(f, "Connection error: {}", err)
            }
            ServiceReceiveError::Logic(msg) => {
                write!(f, "Error: {}", msg)
            }
        }
    }
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct ServiceTag {
    id: i32,
    name: String,
    tag_id: i32,
    value: String,
}

#[derive(serde::Deserialize, serde::Serialize, Debug)]
pub struct ServiceInfo {
    id: i32,
    name: String,
    url: String,
    tags: Vec<ServiceTag>,
}

pub async fn get_services_info(
    config: &KumaConnectionConfig,
    timeout: f32,
) -> Result<Vec<ServiceInfo>, ServiceReceiveError> {
    let (services_info_sender, mut services_info_receiver) = channel::<String>(10);

    let process_service_list = move |payload: Payload, _: Client| {
        let tx_clone = services_info_sender.clone();
        async move {
            match payload {
                Payload::Text(v) => {
                    tracing::debug!("Received service list info");
                    tx_clone
                        .send(serde_json::to_string(&v.clone()).unwrap())
                        .unwrap();
                }
                _ => (),
            }
        }
        .boxed()
    };

    let socket_conn = ClientBuilder::new(config.socket_url.to_string())
        .on("error", |err, _| {
            async move { tracing::debug!("Socket error: {:#?}", err) }.boxed()
        })
        .on("monitorList", process_service_list)
        .transport_type(TransportType::Websocket)
        .connect()
        .await;

    if socket_conn.is_err() {
        return Result::Err(ServiceReceiveError::Conn(socket_conn.err().unwrap()));
    }
    let socket = socket_conn.unwrap();

    time::sleep(time::Duration::from_millis(100)).await;

    tracing::debug!("Attempt to login in {}", config.url);

    let login_req = socket
        .emit_with_ack(
            "login",
            json!({
            "username": config.login,
            "password": config.password,
            "token":""
            }),
            time::Duration::from_secs(4),
            |payload, _| {
                async move {
                    match payload {
                        Payload::Text(_) => tracing::debug!("Successfully logged in kuma"),
                        _ => (),
                    }
                }
                .boxed()
            },
        )
        .await;

    if login_req.is_err() {
        return Result::Err(ServiceReceiveError::Conn(login_req.err().unwrap()));
    }

    let response = tokio::time::timeout(
        time::Duration::from_secs_f32(timeout),
        services_info_receiver.recv(),
    )
    .await;

    if response.is_err() {
        return Err(ServiceReceiveError::Logic(format!(
            "Failed to fetch monitorList after {} seconds",
            timeout
        )));
    }

    match socket.disconnect().await {
        Ok(_) => {}
        Err(reason) => {
            tracing::debug!("Disconnect failed {}", reason)
        }
    }

    let raw_services_info = response.unwrap().unwrap();
    let full_info: Value = serde_json::from_str(raw_services_info.as_str()).unwrap();

    let mut services: Vec<ServiceInfo> = vec![];

    let services_raw = full_info
        .as_array()
        .unwrap()
        .first()
        .unwrap()
        .as_object()
        .unwrap();
    for service_info in services_raw.values() {
        let service: ServiceInfo = serde_json::from_value(service_info.clone()).unwrap();
        services.push(service)
    }
    return Result::Ok(services);
}

pub type ServiceName = String;
pub type TagName = String;

pub type TagMap = HashMap<TagName, Vec<ServiceName>>;

pub fn build_tags_map(services: Vec<ServiceInfo>) -> TagMap {
    let mut tag_map: HashMap<String, Vec<ServiceName>> = HashMap::new();
    for service in services {
        for tag in service.tags {
            if !tag_map.contains_key(&tag.name) {
                let mut service_names: Vec<ServiceName> = vec![];
                service_names.push(service.name.clone());
                tag_map.insert(tag.name, service_names);
            } else {
                let service_names = tag_map.get_mut(&tag.name).unwrap();
                service_names.push(service.name.clone());
            }
        }
    }

    return tag_map;
}

#[tokio::main]
async fn main() {
    dotenv::dotenv().ok();
    let config = Arc::new(KumaConnectionConfig::new());

    let services = get_services_info(&config, 5.).await.unwrap();
    let tag_map = build_tags_map(services);
    println!("{:#?}", tag_map);
}
