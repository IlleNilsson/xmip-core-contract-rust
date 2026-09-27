#![forbid(unsafe_code)]

//! The Rust content contract — a technology of `xmip-core-contract`, as a
//! loadable module rather than a crate the runtime links.
//!
//! ADR-0042 decision 3: a contract may be authored in any declared language
//! over the C ABI, and this is the Rust one. It is what a provider writes: a
//! [`ContractFactory`], and one line naming it to the contract capability's
//! export, which supplies the entry point, the contract table and the
//! lifecycle. No `unsafe` is written here, and a module shipped this way is a
//! separate work under its provider's license (ADR-0061).
//!
//! What it claims: well-formedness is bytes — a Stream is read to its end and
//! held (ADR-0042 decision 1). A descriptor a Location binds is kept and
//! answered by `implies` under `descriptor`. A Rust module with a real
//! standard replaces [`Held::validate`] and nothing else.
//!
//! Until 2026-09-24 this was 395 lines holding the C glue by hand; that glue
//! is the contract capability's now, written once for every Rust contract.

use contract::{
    Contract, ContractDescriptor, ContractError, ContractFactory, ContractId, ValidationResult,
};
use stream::Stream;
use xcore::settings::{Applies, Kind, Presence, Setting, Settings};

/// Every Stream holds: the identity contract.
pub struct Held(ContractDescriptor);

impl Contract for Held {
    fn descriptor(&self) -> &ContractDescriptor {
        &self.0
    }

    fn identify(&self, _stream: &Stream) -> Result<bool, ContractError> {
        Ok(true)
    }

    fn validate(&self, _stream: &Stream) -> Result<ValidationResult, ContractError> {
        Ok(ValidationResult::of(Vec::new()))
    }
}

/// Makes the identity contract from any descriptor.
pub struct Factory;

impl ContractFactory for Factory {
    fn technology(&self) -> &'static str {
        "rust"
    }

    fn settings(&self) -> &'static Settings {
        SETTINGS
    }

    fn load(&self, _reference: &str) -> Result<Box<dyn Contract>, ContractError> {
        Ok(Box::new(Held(ContractDescriptor {
            id: ContractId("rust".to_string()),
            version: "1".to_string(),
            representation: "application/octet-stream".to_string(),
        })))
    }
}

/// What a Location gives this contract (ADR-0064, amendment 2026-09-26).
const SETTINGS: &Settings = &Settings {
    technology: env!("CARGO_PKG_NAME"),
    settings: &[Setting {
        name: "reference",
        kind: Kind::Text,
        presence: Presence::Optional,
        meaning: "Any descriptor; the identity contract holds every Stream whatever it names.",
        applies: Applies::Both,
    }],
};

contract::export_contract!(
    Factory,
    provider = "core",
    standard = "rust",
    version = (0, 1, 0)
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_identity_contract_holds_any_stream_and_names_itself() {
        let contract = Factory.load("any").expect("loaded");
        assert_eq!(contract.descriptor().id.0, "rust");
        let stream = Stream::new(xcore::StreamId::new(1), vec![0xff, 0], None);
        assert!(contract.validate(&stream).expect("judged").valid);
    }

    #[test]
    fn rust_declares_its_settings_and_reads_through_them() {
        assert!(SETTINGS.problems().is_empty(), "{:?}", SETTINGS.problems());
        let given = |name: &str, value: &str| {
            (
                name.to_string(),
                xcore::settings::Given::Text(value.to_string()),
            )
        };
        assert!(Factory.open(Applies::Both, &[]).is_ok(), "bare");
        let bound = Factory
            .open(Applies::Receive, &[given("reference", "any")])
            .expect("bound");
        assert!(bound.descriptor().id.0.contains("rust"));
        let refused = Factory
            .open(Applies::Send, &[given("unheard_of", "x")])
            .err()
            .expect("an unknown setting is refused");
        assert!(
            refused.message.contains("unheard_of"),
            "{}",
            refused.message
        );
    }
}
