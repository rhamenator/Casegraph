//! Domain-package contribution boundary. Core code has no dependency on package implementations.

use casegraph_domain::RuleDefinition;

/// Versioned rule contributed by a domain package.
#[derive(Clone, Debug, PartialEq)]
pub struct RuleContribution {
    pub stable_key: &'static str,
    pub title: &'static str,
    pub version: u32,
    pub definition: RuleDefinition,
}

/// Minimal domain extension contract for the foundation cycle.
pub trait DomainPackage: Send + Sync {
    fn package_id(&self) -> &'static str;
    fn version(&self) -> &'static str;
    fn rules(&self) -> Vec<RuleContribution>;
}

/// Explicit registry; disabling/removing a package leaves this empty without affecting core.
#[derive(Default)]
pub struct DomainRegistry {
    packages: Vec<Box<dyn DomainPackage>>,
}

impl DomainRegistry {
    pub fn new(packages: Vec<Box<dyn DomainPackage>>) -> Self {
        Self { packages }
    }

    pub fn packages(&self) -> impl Iterator<Item = &dyn DomainPackage> {
        self.packages.iter().map(Box::as_ref)
    }
}

#[cfg(test)]
mod tests {
    use super::DomainRegistry;

    #[test]
    fn core_registry_operates_without_any_domain_package() {
        assert_eq!(DomainRegistry::default().packages().count(), 0);
    }
}
