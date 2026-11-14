//! The account's own iroh relay, which every device of the account uses in place of iroh's public
//! ones while it is set.

const MAX_URL_CHARS: usize = 300;

/// The relay's URL as the account keeps it, or why it can't be one.
pub fn checked_url(text: &str) -> Result<String, &'static str> {
    let url = text.trim().trim_end_matches('/');
    let Some(rest) = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://")) else {
        return Err("A relay's address starts with https://.");
    };
    if url.chars().count() > MAX_URL_CHARS {
        return Err("That address is too long for a relay.");
    }
    let host = rest.split(['/', ':', '?', '#']).next().unwrap_or_default();
    if host.is_empty() || url.chars().any(|character| character.is_whitespace() || character.is_control()) {
        return Err("That isn't a relay's address.");
    }
    Ok(url.to_string())
}

/// The relays an endpoint uses for the account's relay: it alone, or iroh's public ones.
#[cfg(feature = "relay")]
pub fn relay_map(relay: Option<&str>) -> iroh::RelayMap {
    match relay.and_then(|url| url.parse::<iroh::RelayUrl>().ok()) {
        Some(url) => iroh::RelayMap::from(url),
        None => iroh::RelayMode::Default.relay_map(),
    }
}

/// Moves the endpoint from the relays of one account relay to another's.
#[cfg(feature = "relay")]
pub async fn follow(endpoint: &iroh::Endpoint, from: Option<&str>, to: Option<&str>) {
    let (from, to) = (relay_map(from), relay_map(to));
    for config in to.relays::<Vec<_>>() {
        endpoint.insert_relay(config.url.clone(), config).await;
    }
    for url in from.urls::<Vec<iroh::RelayUrl>>() {
        if !to.contains(&url) {
            endpoint.remove_relay(&url).await;
        }
    }
}

/// Has an endpoint opened with iroh's public relays follow the account's relay as it changes,
/// until the endpoint closes.
#[cfg(feature = "relay")]
pub fn keep_following(endpoint: iroh::Endpoint, mut relay: tokio::sync::watch::Receiver<Option<String>>) {
    tokio::spawn(async move {
        let mut current = None;
        loop {
            let wanted = relay.borrow_and_update().clone();
            if endpoint.is_closed() {
                return;
            }
            if wanted != current {
                follow(&endpoint, current.as_deref(), wanted.as_deref()).await;
                current = wanted;
            }
            if relay.changed().await.is_err() {
                return;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keeps_a_relay_address_without_its_trailing_slash() {
        assert_eq!(checked_url(" https://relay.example.com/ "), Ok("https://relay.example.com".into()));
        assert_eq!(checked_url("http://10.0.0.2:3340"), Ok("http://10.0.0.2:3340".into()));
    }

    #[test]
    fn refuses_what_isnt_a_relay_address() {
        assert!(checked_url("relay.example.com").is_err());
        assert!(checked_url("https://").is_err());
        assert!(checked_url("https://relay example.com").is_err());
        assert!(checked_url(&format!("https://{}.com", "a".repeat(400))).is_err());
    }
}
