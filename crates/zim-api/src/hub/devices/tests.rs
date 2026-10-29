use serde_json::json;
use zim_crypto::PrivateKey;

use super::*;

fn key() -> PublicKey {
    PrivateKey::generate().public()
}

#[test]
fn alice_sees_her_web_key_as_web_kind() {
    let k = key();
    let wire = json!({
        "pubkey": k.to_hex(),
        "did": Did::from_key(&k).to_string(),
        "label": null,
        "kind": "web",
        "created_at": "2026-01-02T03:04:05Z",
    });

    let d: Device = serde_json::from_value(wire.clone()).unwrap();

    assert_eq!(d.kind, DeviceKind::Web);
    assert_eq!(d.pubkey, k);
    assert_eq!(d.did(), Did::from_key(&k));
    assert_eq!(serde_json::to_value(&d).unwrap(), wire);
}

#[test]
fn bobs_daemon_round_trips_byte_for_byte() {
    let k = key();
    let wire = format!(
        r#"{{"pubkey":"{}","did":"{}","label":"laptop","kind":"daemon","created_at":"2026-01-02T03:04:05Z"}}"#,
        k.to_hex(),
        Did::from_key(&k)
    );

    let d: Device = serde_json::from_str(&wire).unwrap();

    assert_eq!(d.kind, DeviceKind::Daemon);
    assert_eq!(d.label.as_deref(), Some("laptop"));
    assert_eq!(serde_json::to_string(&d).unwrap(), wire);
}

#[test]
fn an_older_hub_without_device_dids_still_parses() {
    let k = key();
    // Empty did, and the field missing entirely, are both "no did".
    let empty =
        json!({"pubkey": k.to_hex(), "did": "", "label": null, "kind": "daemon", "created_at": ""});
    let missing = json!({"pubkey": k.to_hex(), "label": null, "kind": "daemon"});

    let a: Device = serde_json::from_value(empty.clone()).unwrap();
    let b: Device = serde_json::from_value(missing).unwrap();

    assert_eq!(a.did, None);
    assert_eq!(b.did, None);
    assert_eq!(a.created_at, None);
    // The derived did stands in when the hub sent none.
    assert_eq!(a.did(), Did::from_key(&k));
    // ...and the wire stays what the old shape emitted.
    assert_eq!(serde_json::to_value(&a).unwrap(), empty);
}

#[test]
fn a_device_with_a_bad_pubkey_or_kind_is_rejected() {
    let bad_key = json!({"pubkey": "nope", "label": null, "kind": "web"});
    let bad_kind = json!({"pubkey": key().to_hex(), "label": null, "kind": "toaster"});

    assert!(serde_json::from_value::<Device>(bad_key).is_err());
    assert!(serde_json::from_value::<Device>(bad_kind).is_err());
}

#[test]
fn an_unlabeled_device_is_nicked_by_its_key() {
    let k = key();
    let mut d: Device =
        serde_json::from_value(json!({"pubkey": k.to_hex(), "label": null, "kind": "daemon"}))
            .unwrap();
    assert_eq!(d.nick(), format!("hub-{}", &k.to_hex()[..8]));

    d.label = Some("  desk  ".into());
    assert_eq!(d.nick(), "desk");
}
