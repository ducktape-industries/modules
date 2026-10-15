//! The services a node provides, read from a TOML file: the relay, and for each HTTP service the
//! base URL its requests go to and the headers that carry its secrets, each named by the
//! environment variable that holds it.
use std::collections::BTreeMap;

use provider::RELAY;
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct File {
    #[serde(default)]
    relay: bool,
    #[serde(default)]
    services: BTreeMap<String, Service>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Service {
    base: String,
    #[serde(default)]
    secrets: BTreeMap<String, String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Upstream {
    pub base: String,
    pub secrets: Vec<(String, String)>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    pub relay: bool,
    pub upstreams: BTreeMap<String, Upstream>,
}

pub enum Route<'a> {
    Relay,
    Http(&'a Upstream),
    Unserved,
}

impl Config {
    pub fn parse(text: &str, env: impl Fn(&str) -> Option<String>) -> Result<Config, String> {
        let file: File =
            toml::from_str(text).map_err(|error| format!("the config does not parse: {error}"))?;
        if file.services.contains_key(RELAY) {
            return Err(format!(
                "{RELAY} is served with `relay = true`, not as an HTTP service"
            ));
        }
        let mut upstreams = BTreeMap::new();
        for (name, service) in file.services {
            let secrets = service
                .secrets
                .into_iter()
                .map(|(header, variable)| {
                    let value = env(&variable).ok_or_else(|| {
                        format!("{name}'s {header} names {variable}, which is not set")
                    })?;
                    Ok((header, value))
                })
                .collect::<Result<_, String>>()?;
            let upstream = Upstream {
                base: service.base.trim_end_matches('/').to_owned(),
                secrets,
            };
            upstreams.insert(name, upstream);
        }
        Ok(Config {
            relay: file.relay,
            upstreams,
        })
    }

    pub fn services(&self) -> Vec<String> {
        let relay = self.relay.then(|| RELAY.to_owned());
        relay
            .into_iter()
            .chain(self.upstreams.keys().cloned())
            .collect()
    }

    pub fn route(&self, service: &str) -> Route<'_> {
        let relayed = self.relay && service == RELAY;
        if relayed {
            return Route::Relay;
        }
        match self.upstreams.get(service) {
            Some(upstream) => Route::Http(upstream),
            None => Route::Unserved,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEXT: &str = r#"
relay = true

[services.anthropic]
base = "https://api.anthropic.com/"
secrets = { "x-api-key" = "ANTHROPIC_API_KEY" }
"#;

    fn env(variable: &str) -> Option<String> {
        (variable == "ANTHROPIC_API_KEY").then(|| "sk-test".to_owned())
    }

    #[test]
    fn a_config_names_its_services_and_reads_secrets_from_the_environment() {
        let config = Config::parse(TEXT, env).unwrap();
        assert_eq!(config.services(), ["relay", "anthropic"]);
        let Route::Http(upstream) = config.route("anthropic") else {
            panic!("anthropic is an HTTP service");
        };
        assert_eq!(
            upstream,
            &Upstream {
                base: "https://api.anthropic.com".into(),
                secrets: vec![("x-api-key".into(), "sk-test".into())],
            }
        );
        assert!(matches!(config.route("relay"), Route::Relay));
        assert!(matches!(config.route("weather"), Route::Unserved));
    }

    #[test]
    fn a_secret_whose_variable_is_unset_or_a_relay_named_as_http_is_refused() {
        let unset = Config::parse(TEXT, |_| None).unwrap_err();
        assert!(unset.contains("ANTHROPIC_API_KEY"), "{unset}");
        let relay = "[services.relay]\nbase = \"https://example.com\"";
        assert!(Config::parse(relay, env).is_err());
        let quiet = Config::parse("", env).unwrap();
        assert!(quiet.services().is_empty());
        assert!(matches!(quiet.route("relay"), Route::Unserved));
    }
}
