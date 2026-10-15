//! One request answered: the relay at once with nothing, an HTTP service by sending the request's
//! head and its parts, joined, to the service's base URL with the node's secrets, and answering
//! with the whole response it gets back.
use provider::Answer;
use provider::http::{Head, Response};

use crate::config::{Route, Upstream};

pub async fn answer(
    http: &reqwest::Client,
    route: Route<'_>,
    input: &[u8],
    body: Vec<u8>,
) -> Answer {
    let upstream = match route {
        Route::Relay => return Answer::Output(Vec::new()),
        Route::Unserved => return Answer::Failed("this node does not serve it".to_owned()),
        Route::Http(upstream) => upstream,
    };
    let sent = match abi::decode::<Head>(input) {
        Ok(head) => send(http, upstream, &head, body).await,
        Err(refusal) => Err(format!(
            "the input is not an HTTP head: {}",
            refusal.sentence
        )),
    };
    match sent {
        Ok(response) => Answer::Output(abi::encode(&response)),
        Err(reason) => Answer::Failed(reason),
    }
}

async fn send(
    http: &reqwest::Client,
    upstream: &Upstream,
    head: &Head,
    body: Vec<u8>,
) -> Result<Response, String> {
    head.check().map_err(|error| error.message)?;
    let method = reqwest::Method::from_bytes(head.method.as_str().as_bytes())
        .map_err(|error| error.to_string())?;
    let url = format!("{}{}", upstream.base, head.path);
    let mut request = http.request(method, url);
    for (name, value) in head.headers.iter().chain(&upstream.secrets) {
        request = request.header(name, value);
    }
    if !body.is_empty() {
        request = request.body(body);
    }
    let response = request
        .send()
        .await
        .map_err(|error| format!("the service was not reached: {error}"))?;
    let status = response.status().as_u16();
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| {
            Some((name.as_str().to_owned(), value.to_str().ok()?.to_owned()))
        })
        .collect();
    let body = response
        .bytes()
        .await
        .map_err(|error| format!("the service's answer broke off: {error}"))?
        .to_vec();
    Ok(Response {
        status,
        headers,
        body,
    })
}

#[cfg(test)]
mod tests {
    use axum::Router;
    use axum::http::{HeaderMap, Method as Verb, Uri};
    use axum::routing::any;
    use provider::http::Method;
    use tokio::net::TcpListener;

    use super::*;

    async fn echo(verb: Verb, uri: Uri, headers: HeaderMap, body: String) -> (HeaderMap, String) {
        let key = headers["x-api-key"].to_str().unwrap();
        let version = headers["anthropic-version"].to_str().unwrap();
        let mut answered = HeaderMap::new();
        answered.insert("x-seen", "yes".parse().unwrap());
        (answered, format!("{verb} {uri} {key} {version} {body}"))
    }

    async fn upstream() -> Upstream {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let app = Router::new().fallback(any(echo));
        tokio::spawn(async move { axum::serve(listener, app).await.unwrap() });
        Upstream {
            base: format!("http://{address}"),
            secrets: vec![("x-api-key".into(), "sk-test".into())],
        }
    }

    fn head(headers: Vec<(&str, &str)>) -> Vec<u8> {
        abi::encode(&Head {
            method: Method::Post,
            path: "/v1/messages".into(),
            headers: headers
                .into_iter()
                .map(|(name, value)| (name.into(), value.into()))
                .collect(),
        })
    }

    #[tokio::test]
    async fn a_request_reaches_its_service_with_the_nodes_secret_and_the_whole_response_comes_back()
    {
        let upstream = upstream().await;
        let http = reqwest::Client::new();
        let input = head(vec![("anthropic-version", "2023-06-01")]);
        let answer = answer(&http, Route::Http(&upstream), &input, b"{\"a\":1}".to_vec()).await;
        let Answer::Output(bytes) = answer else {
            panic!("answered {answer:?}");
        };
        let response: Response = abi::decode(&bytes).unwrap();
        assert_eq!(response.status, 200);
        assert_eq!(
            String::from_utf8(response.body).unwrap(),
            "POST /v1/messages sk-test 2023-06-01 {\"a\":1}"
        );
        assert!(response.headers.contains(&("x-seen".into(), "yes".into())));
    }

    #[tokio::test]
    async fn a_request_naming_a_secret_header_or_no_head_fails_and_the_relay_answers_empty() {
        let upstream = upstream().await;
        let http = reqwest::Client::new();
        let smuggled = head(vec![("x-api-key", "theirs")]);
        let refused = answer(&http, Route::Http(&upstream), &smuggled, Vec::new()).await;
        assert!(
            matches!(&refused, Answer::Failed(reason) if reason.contains("x-api-key")),
            "{refused:?}"
        );
        let garbage = answer(&http, Route::Http(&upstream), b"\xff", Vec::new()).await;
        assert!(matches!(&garbage, Answer::Failed(reason) if reason.contains("not an HTTP head")));
        let relayed = answer(&http, Route::Relay, b"toolu_1", Vec::new()).await;
        assert_eq!(relayed, Answer::Output(Vec::new()));
        let unserved = answer(&http, Route::Unserved, b"", Vec::new()).await;
        assert!(matches!(unserved, Answer::Failed(_)));
    }
}
