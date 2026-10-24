//! "A new version is available" notice.
//!
//! The newest release is read from where `github.com/<repo>/releases/latest`
//! redirects (the tag), not from the REST API, which allows only 60
//! anonymous requests per hour per address. Nothing is downloaded or
//! installed: the app only tells the user and links to the release page.

use std::time::Duration;

pub const RELEASES_URL: &str = "https://github.com/fabandrade88/joshuaplus/releases";

/// `major.minor.patch` and whether it is a pre-release (`-beta`, `-rc.1`…).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Version {
    pub major: u64,
    pub minor: u64,
    pub patch: u64,
    pub pre: bool,
}

/// Parses `v1.2.3`, `1.2.3` or `1.2.3-beta`.
#[must_use]
pub fn parse_version(s: &str) -> Option<Version> {
    let s = s.trim().trim_start_matches('v');
    let (core, pre) = match s.split_once('-') {
        Some((core, tail)) if !tail.is_empty() => (core, true),
        Some(_) => return None,
        None => (s, false),
    };
    let mut parts = core.split('.').map(|p| p.parse::<u64>().ok());
    let v = Version { major: parts.next()??, minor: parts.next()??, patch: parts.next()??, pre };
    parts.next().is_none().then_some(v)
}

/// `latest` is a newer release than `current` (a final release beats the
/// pre-release of the same number).
#[must_use]
pub fn is_newer(latest: &str, current: &str) -> bool {
    match (parse_version(latest), parse_version(current)) {
        (Some(l), Some(c)) => (l.major, l.minor, l.patch, !l.pre) > (c.major, c.minor, c.patch, !c.pre),
        _ => false,
    }
}

/// Tag of the latest published release (e.g. `v0.6.0`).
///
/// # Errors
/// Network failure or an unexpected answer.
pub fn latest_tag() -> Result<String, String> {
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(8)))
        .max_redirects(0)
        .http_status_as_error(false)
        .user_agent("joshua-plus-update-check")
        .build()
        .into();
    let response = agent.head(&format!("{RELEASES_URL}/latest")).call().map_err(|e| e.to_string())?;
    let location = response
        .headers()
        .get("location")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| format!("no redirect (HTTP {})", response.status()))?;
    tag_from_location(location).ok_or_else(|| "no published release".to_owned())
}

fn tag_from_location(location: &str) -> Option<String> {
    let tag = location.split("/tag/").nth(1)?.split(['?', '#', '/']).next()?;
    parse_version(tag).map(|_| tag.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn versions_compare_like_releases() {
        assert!(is_newer("v0.6.0", "0.5.0"));
        assert!(is_newer("v1.0.0", "0.9.9"));
        assert!(is_newer("v0.5.1", "0.5.0"));
        assert!(is_newer("v0.5.0", "0.5.0-beta"), "the final release beats its beta");
        assert!(!is_newer("v0.5.0", "0.5.0"));
        assert!(!is_newer("v0.4.9", "0.5.0"));
        assert!(!is_newer("v0.6.0-beta", "0.6.0"));
        assert!(!is_newer("garbage", "0.5.0"));
        assert!(!is_newer("v0.6", "0.5.0"));
        assert!(!is_newer("v0.6.0.1", "0.5.0"));
    }

    #[test]
    fn the_tag_comes_from_the_release_redirect() {
        let base = "https://github.com/fabandrade88/joshuaplus/releases/tag/";
        assert_eq!(tag_from_location(&format!("{base}v0.6.0")).as_deref(), Some("v0.6.0"));
        assert_eq!(tag_from_location(&format!("{base}v1.2.3-rc.1?x=1")).as_deref(), Some("v1.2.3-rc.1"));
        assert_eq!(tag_from_location("https://github.com/fabandrade88/joshuaplus/releases"), None);
        assert_eq!(tag_from_location(&format!("{base}not-a-version")), None);
    }
}
