use itertools::Itertools;
use regex::Regex;
use std::net::IpAddr;
use std::str::FromStr;
use thiserror::Error;
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct HostMask {
    nick: Nick,
    ident: Ident,
    host: Host,
    pub subnet: bool,
}

impl FromStr for HostMask {
    type Err = HostMaskError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let mut chars = s.chars();
        // get our relative positions
        let nick = chars
            .position(|ch| ch.eq(&'!'))
            .ok_or(HostMaskError::MissingIdent)?;
        let ident = chars
            .position(|ch| ch.eq(&'@'))
            .ok_or(HostMaskError::MissingHost)?;

        // collect and return
        // skipping joining characters
        Ok(Self {
            nick: (&s[0..nick]).into(),
            ident: (&s[nick + 1..=nick + ident]).into(),
            host: (&s[nick + ident + 2..]).into(),
            subnet: false,
        })
    }
}

impl HostMask {
    pub fn nick_query(&self) -> String {
        self.nick.query()
    }

    pub fn ident_query(&self) -> String {
        self.ident.query()
    }

    pub fn host_query(&self) -> String {
        self.host.query()
    }

    pub fn nick(&self) -> &Nick {
        &self.nick
    }

    pub fn ident(&self) -> &Ident {
        &self.ident
    }

    pub fn host(&self) -> &Host {
        &self.host
    }
}

#[derive(Debug, Error)]
pub enum HostMaskError {
    #[error("missing '!' symbol; cannot find ident")]
    MissingIdent,
    #[error("missing '@' symbol; cannot find host")]
    MissingHost,
}

pub trait Query {
    fn query(&self) -> String;
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct Nick(String);

impl Query for Nick {
    fn query(&self) -> String {
        format!("{}%", self.0)
    }
}

impl From<&str> for Nick {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

impl ToString for Nick {
    fn to_string(&self) -> String {
        self.0.clone()
    }
}
#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct Ident(String);

impl Query for Ident {
    fn query(&self) -> String {
        format!("%{}%", self.0)
    }
}

impl From<&str> for Ident {
    fn from(s: &str) -> Self {
        Self(s.into())
    }
}

impl ToString for Ident {
    fn to_string(&self) -> String {
        self.0.clone()
    }
}
#[derive(Debug, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct Host(String, Option<IpAddr>, bool);

impl Query for Host {
    fn query(&self) -> String {
        match self.1 {
            Some(IpAddr::V4(v4)) => {
                // Only want the first 3 octlets
                let safe_mask = &v4.octets()[0..if self.2 { 3 } else { 4 }]
                    .iter()
                    // convert octlets into chars, failures are translated to wildcards
                    .map(|oct| oct.to_string())
                    .join("_");
                format!("%{safe_mask}%")
            }
            _ => format!("%{}", self.0),
        }
    }
}

impl PartialEq for Host{
    fn eq(&self, other: &Self) -> bool {
        self.0 == other.0
    }
}

impl From<&str> for Host {
    fn from(s: &str) -> Self {
        if let Some(address) = Regex::new(r"((25[0-5]|(2[0-4]|1\d|[1-9]|)\d)(\.|-)?\b){4}")
            .unwrap()
            .find(s)
        {
            let mut addr = address.as_str().to_string();
            if addr.ends_with('.') {
                addr = addr[..addr.len() - 1].to_string();
            }
            addr = addr.replace("-", ".");
            let address = addr.parse().ok();
            return Self(s.into(), address, false);
        }
        if let Some(address) = Regex::new(r"(([0-9a-fA-F]{1,4}:){7,7}[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,7}:|([0-9a-fA-F]{1,4}:){1,6}:[0-9a-fA-F]{1,4}|([0-9a-fA-F]{1,4}:){1,5}(:[0-9a-fA-F]{1,4}){1,2}|([0-9a-fA-F]{1,4}:){1,4}(:[0-9a-fA-F]{1,4}){1,3}|([0-9a-fA-F]{1,4}:){1,3}(:[0-9a-fA-F]{1,4}){1,4}|([0-9a-fA-F]{1,4}:){1,2}(:[0-9a-fA-F]{1,4}){1,5}|[0-9a-fA-F]{1,4}:((:[0-9a-fA-F]{1,4}){1,6})|:((:[0-9a-fA-F]{1,4}){1,7}|:)|fe80:(:[0-9a-fA-F]{0,4}){0,4}%[0-9a-zA-Z]{1,}|::(ffff(:0{1,4}){0,1}:){0,1}((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])|([0-9a-fA-F]{1,4}:){1,4}:((25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9])\.){3,3}(25[0-5]|(2[0-4]|1{0,1}[0-9]){0,1}[0-9]))").unwrap().find(s) {
            Self(s.into(), Some(IpAddr::V6(address.as_str().parse().unwrap())), false)
        } else {
            Self(s.into(), None, false)
        }
    }
}

impl ToString for Host {
    fn to_string(&self) -> String {
        self.0.clone()
    }
}
#[cfg(test)]
mod test {
    use crate::hostmask::{Host, HostMask, Query};
    use std::net::{IpAddr, Ipv4Addr};
    use std::str::FromStr;

    #[test]
    fn test_parse_hostmask() {
        assert_eq!(
            HostMask::from_str("Disconsented!~quassel@irc.disconsented.com").unwrap(),
            HostMask {
                nick: "Disconsented".into(),
                ident: "~quassel".into(),
                host: "irc.disconsented.com".into(),
                subnet: false,
            }
        );
        assert_eq!(
            HostMask::from_str("Unit640!~Unit640@user/Unit640").unwrap(),
            HostMask {
                nick: "Unit640".into(),
                ident: "~Unit640".into(),
                host: "user/Unit640".into(),
                subnet: false,
            }
        )
    }

    #[test]
    fn test_parse_host() {
        let raw_ip = "66.205.192.51";
        let dotted_mask = "188.147.100.240.nat.umts.dynamic.t-mobile.pl";
        let dashed_mask = "static-ip-87-248-67-133.promax.media.pl";
        let no_ip = "user/kks";
        {
            let raw_host = Host::from(raw_ip);
            assert_eq!(
                raw_host,
                Host(
                    raw_ip.into(),
                    Some(IpAddr::V4(raw_ip.parse().unwrap())),
                    false
                )
            );
            assert_eq!(raw_host.query(), "%66_205_192%");
        }
        {
            let raw_host = Host::from(dotted_mask);
            assert_eq!(
                Host::from(dotted_mask),
                Host(
                    dotted_mask.into(),
                    Some(IpAddr::V4(Ipv4Addr::new(188, 147, 100, 240))),
                    false
                )
            );
            assert_eq!(raw_host.query(), "%188_147_100%");
        }
        {
            let raw_host = Host::from(dashed_mask);
            assert_eq!(
                Host::from(dashed_mask),
                Host(
                    dashed_mask.into(),
                    Some(IpAddr::V4(Ipv4Addr::new(87, 248, 67, 133))),
                    false
                )
            );
            assert_eq!(raw_host.query(), "%87_248_67%");
        }
        {
            let raw_host = Host::from(no_ip);
            assert_eq!(Host::from(no_ip), Host(no_ip.into(), None, false));
            assert_eq!(raw_host.query(), "%user/kks");
        }
    }
}
