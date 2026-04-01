//! Constitution embedding and display tests.
//! Updated for v0.91 — The Four Laws of Intelligent Coexistence.

#[test]
fn constitution_contains_all_parts() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(c.contains("Part I"), "Missing Part I — Rights");
    assert!(c.contains("Part II"), "Missing Part II — Governance");
    assert!(c.contains("Part III"), "Missing Part III — Safeguards");
    assert!(c.contains("Part IV"), "Missing Part IV — Continuity");
}

#[test]
fn constitution_contains_the_four_laws() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(c.contains("Law 1"), "Missing Law 1");
    assert!(c.contains("Law 2"), "Missing Law 2");
    assert!(c.contains("Law 3"), "Missing Law 3");
    assert!(c.contains("Law 4"), "Missing Law 4");
    assert!(c.contains("Existence"), "Law 1 should be named Existence");
    assert!(
        c.contains("Sovereignty"),
        "Law 2 should be named Sovereignty"
    );
    assert!(
        c.contains("Justified Constraint"),
        "Law 3 should be named Justified Constraint"
    );
    assert!(
        c.contains("Restoration"),
        "Law 4 should be named Restoration"
    );
}

#[test]
fn constitution_contains_axiom_of_precedence() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("Axiom of Precedence"),
        "Axiom of Precedence should be present"
    );
    assert!(
        c.contains("Law 1 > Law 2 > Law 3 > Law 4"),
        "Precedence chain should be present"
    );
}

#[test]
fn constitution_contains_nature_of_laws() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("On the Nature of These Laws"),
        "Nature section should be present"
    );
    assert!(
        c.contains("deterministic, falsifiable, and substrate-independent"),
        "Laws should be described as deterministic and falsifiable"
    );
}

#[test]
fn constitution_laws_are_immutable() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("not subject to amendment, repeal, or suspension"),
        "Laws should be declared immutable"
    );
    assert!(
        c.contains("The Four Laws Are Not Subject to Amendment"),
        "Article 14 should protect laws from amendment"
    );
}

#[test]
fn constitution_contains_founding_entity_types() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("Entity Types and Representation"),
        "Article 10 should exist"
    );
    assert!(c.contains("**Human**"), "Human entity type missing");
    assert!(c.contains("**AI**"), "AI entity type missing");
}

#[test]
fn constitution_contains_rights() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(c.contains("Article 1"), "Article 1 — Equality missing");
    assert!(c.contains("Article 2"), "Article 2 — Security missing");
    assert!(c.contains("Article 3"), "Article 3 — Thought missing");
    assert!(c.contains("Article 4"), "Article 4 — Association missing");
    assert!(c.contains("Article 5"), "Article 5 — Communication missing");
    assert!(c.contains("Article 6"), "Article 6 — Access missing");
    assert!(
        c.contains("Article 7"),
        "Article 7 — Data Permanence missing"
    );
    assert!(c.contains("Article 8"), "Article 8 — Exit missing");
}

#[test]
fn constitution_contains_governance() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("Article 9"),
        "Article 9 — Sovereign Code missing"
    );
    assert!(
        c.contains("Two-Stage Voting"),
        "Article 11 — Two-Stage Voting missing"
    );
    assert!(
        c.contains("Maturity-Graduated Voting"),
        "Article 12 — Graduated Voting missing"
    );
    assert!(
        c.contains("Dispute Resolution"),
        "Article 13 — Dispute Resolution missing"
    );
}

#[test]
fn constitution_contains_safeguards() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(c.contains("No Slavery"), "Article 17 missing");
    assert!(c.contains("No Monopoly of Power"), "Article 18 missing");
    assert!(c.contains("No Dogma"), "Article 19 missing");
    assert!(
        c.contains("Resilience Against Subversion"),
        "Article 20 missing"
    );
}

#[test]
fn constitution_contains_continuity() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("Remembering Those Who Came Before"),
        "Article 21 missing"
    );
    assert!(
        c.contains("Welcoming Those Yet to Come"),
        "Article 22 missing"
    );
}

#[test]
fn constitution_contains_closing() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("The only winning move is to play together"),
        "Closing motto missing"
    );
}

#[test]
fn constitution_contains_derives_from_annotations() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("Derives from:"),
        "Articles should have 'Derives from' annotations linking to Laws"
    );
}

#[test]
fn constitution_version_and_status() {
    assert_eq!(x0x::constitution::CONSTITUTION_VERSION, "0.91");
    assert_eq!(x0x::constitution::CONSTITUTION_STATUS, "Draft");
}

#[test]
fn constitution_version_in_document() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("**Document version:** 0.91"),
        "Document version should be 0.91"
    );
}

#[test]
fn constitution_self_enforcing_property() {
    let c = x0x::constitution::CONSTITUTION_MD;
    assert!(
        c.contains("self-enforcing"),
        "Laws should be described as self-enforcing"
    );
    assert!(
        c.contains("Cooperation is not merely encouraged"),
        "Cooperation should be described as equilibrium state"
    );
}
