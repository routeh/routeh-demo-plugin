//! The manifest declares one of everything, as the router reads it.
use crate::support::{manifest, settings};
use routeh_plugin_sdk::protocol::{FieldType, Reference, StatusType};

#[test]
fn it_is_a_plugin_with_a_module_a_page_and_a_tunnel() {
    let manifest = manifest();
    assert_eq!(manifest.id.as_str(), "demo");
    assert_eq!(manifest.modules, ["demo"]);
    assert_eq!(manifest.service.command, "bin/demo");
    assert_eq!(manifest.page.as_ref().unwrap().title, "Demo");
    assert_eq!(manifest.tunnel.as_ref().unwrap().link_kind, demo::KIND);
    assert_eq!(manifest.actions.len(), 6);
}

#[test]
fn every_type_of_field_and_status_value_is_declared() {
    let manifest = manifest();
    let schema = &manifest.settings;
    let mut fields: Vec<&'static str> = schema
        .keys()
        .map(|key| kind(&schema.field(key).unwrap().kind))
        .collect();
    fields.sort_unstable();
    fields.dedup();
    assert_eq!(
        fields,
        [
            "address",
            "bool",
            "bytes",
            "choice",
            "client",
            "duration",
            "group",
            "host_port",
            "integer",
            "ipv4_prefix",
            "ipv6_prefix",
            "list",
            "network",
            "port",
            "range",
            "template",
            "text",
            "tunnel",
            "uplink",
        ]
    );
    let mut types: Vec<StatusType> = manifest.status.iter().map(|status| status.kind).collect();
    types.extend(
        manifest
            .tunnel
            .unwrap()
            .status
            .iter()
            .map(|status| status.kind),
    );
    for kind in [
        StatusType::Text,
        StatusType::Count,
        StatusType::Bytes,
        StatusType::Duration,
        StatusType::Time,
        StatusType::Address,
    ] {
        assert!(types.contains(&kind), "{kind:?} is not declared");
    }
}

fn kind(kind: &FieldType) -> &'static str {
    match kind {
        FieldType::Text { .. } => "text",
        FieldType::Template { .. } => "template",
        FieldType::Bytes { .. } => "bytes",
        FieldType::Bool => "bool",
        FieldType::Integer { .. } => "integer",
        FieldType::Range { .. } => "range",
        FieldType::Duration { .. } => "duration",
        FieldType::Port => "port",
        FieldType::HostPort => "host_port",
        FieldType::Ipv4Prefix => "ipv4_prefix",
        FieldType::Ipv6Prefix => "ipv6_prefix",
        FieldType::Address => "address",
        FieldType::Choice { .. } => "choice",
        FieldType::Reference(Reference::Uplink) => "uplink",
        FieldType::Reference(Reference::Network) => "network",
        FieldType::Reference(Reference::Tunnel) => "tunnel",
        FieldType::Reference(Reference::Client) => "client",
        FieldType::List { .. } => "list",
        FieldType::Group { .. } => "group",
    }
}

#[test]
fn a_document_that_sets_every_field_is_taken() {
    settings(EVERY_FIELD);
}

/// Every field of the demo's settings, as a document would set them.
pub const EVERY_FIELD: &str = r#"
greeting = "Hi"
log_level = "debug"
heartbeat = "30s"
settle_after = "2s"
text = "abc"
secret = "s3cret"
template = "line one\nline two"
key = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
enabled = true
small = 5
medium = 1000
large = 70000
ports = "2000-3000"
interval = "10m"
port = 8080
server = "vpn.example.com:51820"
subnet4 = "10.1.0.0/16"
subnet6 = "fd00:1::/48"
address = "192.0.2.7"
color = "green"
uplink = "wan"
network = "lan"
tunnel = "vpn"
client = "laptop"
names = ["a", "b"]
addresses = ["192.0.2.1", "2001:db8::1"]
uplinks = ["wan", "wan2"]
records = [{ host = "home", target = "wan", via = "vpn", owner = "laptop", port = 22, enabled = true }]
"#;
