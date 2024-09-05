use reqwest::Url;

pub fn build_url_with_auth(url: &Url, token: &str) -> Url {
    let mut url = url.clone();
    url.set_password(Some(token)).unwrap();

    return url;
}