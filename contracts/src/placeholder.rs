//! Placeholder contract for S0 scaffold smoke test.
//!
//! TODO(S1): replace with `OhuVault` — custodia en `purse` con depósito + transferencia.

use odra::prelude::*;

/// Minimal contract used only to prove the Odra toolchain builds and tests.
#[odra::module]
pub struct Placeholder {
    counter: Var<u32>,
}

#[odra::module]
impl Placeholder {
    pub fn init(&mut self) {
        self.counter.set(0);
    }

    pub fn get(&self) -> u32 {
        self.counter.get_or_default()
    }

    pub fn increment(&mut self) {
        self.counter.set(self.get().checked_add(1).unwrap_or(0));
    }
}

#[cfg(test)]
mod tests {
    use super::Placeholder;
    use odra::host::{Deployer, NoArgs};

    #[test]
    fn placeholder_smoke() {
        let env = odra_test::env();
        let mut contract = Placeholder::deploy(&env, NoArgs);
        assert_eq!(contract.get(), 0);
        contract.increment();
        assert_eq!(contract.get(), 1);
    }
}
