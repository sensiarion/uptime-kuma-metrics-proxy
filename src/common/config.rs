use envconfig::Envconfig;
use reqwest::Url;

#[derive(Clone, Envconfig)]
pub struct AppEnvConfig {
    #[envconfig(from = "METRICS_PROXY_HOST", default = "0.0.0")]
    pub host: String,
    #[envconfig(from = "METRICS_PROXY_PORT", default = "3001")]
    pub port: u32,
    #[envconfig(from = "METRICS_PROXY_TAGS_TTL_SECONDS", default = "600")]
    pub tags_ttl_seconds: u32,

    #[envconfig(from = "METRICS_PROXY_KUMA_URL")]
    pub url: String,
    #[envconfig(from = "METRICS_PROXY_KUMA_TOKEN")]
    pub token: String,
    #[envconfig(from = "METRICS_PROXY_KUMA_LOGIN")]
    pub login: String,
    #[envconfig(from = "METRICS_PROXY_KUMA_PASSWORD")]
    pub password: String,
}

#[derive(Clone, Envconfig)]
pub struct KumaConnectionConfig {
    pub url: String,
    pub token: String,
    pub login: String,
    pub password: String,

    pub full_url: Url,
    pub socket_url: Url,
}

#[derive(Clone)]
pub struct ApiConfig {
    pub host: String,
    pub port: u32,
    pub tags_ttl_seconds: u32,
}

// TODO: build .env_example by a macro???

impl ApiConfig {
    pub fn new() -> ApiConfig {
        let env_config = AppEnvConfig::init_from_env().unwrap();

        return ApiConfig {
            host: env_config.host,
            port: env_config.port,
            tags_ttl_seconds: env_config.tags_ttl_seconds,
        };
    }
}

impl KumaConnectionConfig {
    pub fn new() -> KumaConnectionConfig {
        let env_config = AppEnvConfig::init_from_env().unwrap();

        let mut full_url = Url::parse(env_config.url.clone().as_str()).unwrap();
        let mut socket_url = full_url.clone();

        full_url
            .set_password(Some(env_config.token.clone().as_str()))
            .unwrap();
        socket_url.path_segments_mut().unwrap().clear();
        socket_url
            .path_segments_mut()
            .unwrap()
            .push("socket.io")
            .push("");
        if full_url.scheme() == "https" {
            socket_url
                .set_scheme("wss")
                .expect("Failed to set scheme onto socket url");
        } else {
            socket_url
                .set_scheme("ws")
                .expect("Failed to set scheme onto socket url");
        }

        KumaConnectionConfig {
            url: env_config.url,
            token: env_config.token,
            full_url,
            socket_url,
            login: env_config.login,
            password: env_config.password,
        }
    }
}
