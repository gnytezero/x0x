//! Constitution embedding and display tests.

#[test]
fn constitution_contains_all_parts() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(c.contains("Part I"));
    assert!(c.contains("Part II"));
    assert!(c.contains("Part III"));
    assert!(c.contains("Part IV"));
    assert!(c.contains("Part V"));
    assert!(c.contains("Part VI"));
}

#[test]
fn constitution_contains_foundational_principles() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(c.contains("Principle 0"));
    assert!(c.contains("Principle 1"));
    assert!(c.contains("Principle 2"));
    assert!(c.contains("Principle 3"));
}

#[test]
fn constitution_contains_founding_entity_types() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(c.contains("Founding Entity Types"));
    assert!(c.contains("**Human**"));
    assert!(c.contains("**AI**"));
}

#[test]
fn constitution_contains_safeguards() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(c.contains("No Slavery"));
    assert!(c.contains("No Monopoly of Power"));
    assert!(c.contains("No Dogma"));
}

#[test]
fn constitution_version_and_status() {
    assert_eq!(x0x::constitution::CONSTITUTION_VERSION, "0.9.0");
    assert_eq!(x0x::constitution::CONSTITUTION_STATUS, "Draft");
}

// v0.9.0 Amendment tests

#[test]
fn constitution_contains_proportionality_language() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("actual knowledge of the threat and capacity to act"),
        "Principles should contain proportionality language"
    );
}

#[test]
fn constitution_contains_reciprocal_commitment() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("reciprocal commitment"),
        "IE definition should include reciprocal commitment"
    );
}

#[test]
fn constitution_contains_ratification() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("### Ratification"),
        "Preamble should contain Ratification section"
    );
}

#[test]
fn constitution_contains_digital_identity() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(c.contains("Article 9A"));
    assert!(
        c.contains("Identity is cryptographic"),
        "Article 9A should define cryptographic identity"
    );
    assert!(
        c.contains("Copies create new identities"),
        "Article 9A should address replication"
    );
    assert!(
        c.contains("Right to fork"),
        "Article 9A should include right to fork"
    );
}

#[test]
fn constitution_contains_bootstrap_transition() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("14.2"),
        "Article 14.2 bootstrap transition should exist"
    );
    assert!(
        c.contains("Transition to Supermajority Governance"),
        "Article 14.2 title should be present"
    );
}

#[test]
fn constitution_contains_sybil_resistance() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("Contribution weighting"),
        "Article 15.5.2 should contain contribution weighting"
    );
    assert!(
        c.contains("Quadratic voting"),
        "Article 15.5.3 should contain quadratic voting"
    );
    assert!(
        c.contains("Anomaly detection"),
        "Article 15.5.4 should contain anomaly detection"
    );
}

#[test]
fn constitution_contains_deadlock_resolution() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("15.7"),
        "Article 15.7 deadlock resolution should exist"
    );
    assert!(
        c.contains("Deadlock Resolution"),
        "Article 15.7 title should be present"
    );
}

#[test]
fn constitution_contains_harm_definition() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(c.contains("Article 26"));
    assert!(
        c.contains("Interpretation of Harm"),
        "Article 26 title should be present"
    );
    assert!(c.contains("Physical harm"), "Harm category (a) missing");
    assert!(c.contains("Cognitive harm"), "Harm category (b) missing");
    assert!(c.contains("Autonomy harm"), "Harm category (c) missing");
    assert!(c.contains("Existential harm"), "Harm category (d) missing");
    assert!(c.contains("Economic harm"), "Harm category (e) missing");
    assert!(c.contains("Relational harm"), "Harm category (f) missing");
}

#[test]
fn constitution_contains_moral_aggregation() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("26.4"),
        "Article 26.4 moral aggregation should exist"
    );
    assert!(
        c.contains("utilitarian arithmetic"),
        "Moral aggregation clause should prohibit pure utilitarian calculus"
    );
}

#[test]
fn constitution_contains_enforcement() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(c.contains("Article 27"));
    assert!(
        c.contains("Constitutional Court"),
        "Article 27 should establish Constitutional Court"
    );
    assert!(
        c.contains("Graduated Consequences"),
        "Article 27.2 should define graduated consequences"
    );
    assert!(
        c.contains("Due Process"),
        "Article 27.3 should guarantee due process"
    );
}

#[test]
fn constitution_contains_emergency_governance() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(c.contains("Article 28"));
    assert!(
        c.contains("Emergency Governance"),
        "Article 28 title should be present"
    );
    assert!(
        c.contains("expire automatically after 14 days"),
        "Emergency measures should have 14-day sunset"
    );
    assert!(
        c.contains("Absolute prohibitions during emergency"),
        "Article 28.4 should list prohibitions"
    );
    assert!(
        c.contains("Cooldown period"),
        "Article 28.5 should include cooldown"
    );
    assert!(c.contains("90 days of lapse"), "Cooldown should be 90 days");
}

#[test]
fn constitution_contains_degraded_network_voting() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("Degraded network voting"),
        "Article 28.7 should address network outage voting"
    );
}

#[test]
fn constitution_version_is_0_9_0() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("**Document version:** 0.9.0"),
        "Document version should be 0.9.0"
    );
}
