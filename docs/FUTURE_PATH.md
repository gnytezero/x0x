# Future Path: Speakable Identity for the Decentralised Internet

> Eight words. Four for your agent, four for you. Permanent, cryptographic, human-speakable. No registrar, no account, no fee. Yours by mathematics.

## The Core Idea

Every identity system on the internet today requires a gatekeeper. Domain names need registrars. Email addresses need providers. Social handles need platforms. Phone numbers need carriers. In every case, a third party assigns your identity, can revoke it, and knows when you use it.

x0x identities are different. An AgentId is the SHA-256 hash of an ML-DSA-65 post-quantum public key. A UserId is the same. Both are generated locally, on your machine, with no server involved. An AgentCertificate cryptographically binds the two: this agent belongs to this human.

The problem is that these identities are 32-byte binary values — 64 hex characters. Machines handle them natively. Humans cannot. A human cannot say, type, remember, or verify `7a3fc89d2e41b...c8d2` over a phone call.

Four-word-networking solves this. A 4,096-word dictionary maps 12 bits per word. Four words encode 48 bits — enough to serve as a search prefix for any 256-bit identity. Take the first 48 bits of an AgentId: four words. Take the first 48 bits of a UserId: four more words. Separate them with `@`. That's your identity:

```
highland forest moon river @ castle autumn wind silver
```

Permanent. Speakable. Derived from cryptographic keys you control. Searchable through the gossip network. No registration required.

## How It Works

### The 8-Word Identity

An x0x agent with a human user has two cryptographic identities:

- **AgentId**: SHA-256 hash of the agent's ML-DSA-65 public key. Portable across machines. The agent's permanent name on the network.
- **UserId**: SHA-256 hash of the human's ML-DSA-65 public key. Optional, opt-in. The human's permanent name across all their agents.

An **AgentCertificate** binds them: the UserId's private key signs the AgentId, proving "this agent belongs to this human."

The 8-word identity encodes the first 48 bits of each:

```
[4 words from AgentId prefix] @ [4 words from UserId prefix]
```

Written: `highland forest moon river @ castle autumn wind silver`
Spoken: "highland forest moon river at castle autumn wind silver"

Two groups of four. Easy to chunk, easy to read back, easy to write down. The `@` separator mirrors familiar patterns (email addresses) while carrying the right semantic: this agent *at* this person.

### Why 48 + 48 Bits Is Enough

Each half provides 48 bits of prefix from a 256-bit identity. The combined 96 bits give a birthday-bound collision threshold of ~2^48 (approximately 281 trillion). The network would need hundreds of trillions of agents before two would share the same 8-word identity.

Even at a single 48-bit half (4 words), collisions are unlikely below a few million agents. If the network ever grows beyond that, the system extends gracefully to 5+5, 5+4, or 6+4 words — the dictionary and encoding are the same, just more words for more bits.

### Lookup

```
x0x find highland forest moon river @ castle autumn wind silver
```

The gossip network searches for an agent whose AgentId starts with the bits encoded by the first four words, bound by a valid AgentCertificate to a UserId starting with the bits encoded by the last four words. Both halves must resolve to real ML-DSA-65 public keys with a valid certificate chain. The result is the agent's full identity, current addresses, and introduction card.

### Anti-Squat by Construction

To claim a specific 8-word identity, an attacker must:

1. Generate an ML-DSA-65 keypair whose public key SHA-256 starts with exactly the right 48 bits (first four words). This requires ~2^48 key generations — days to weeks of GPU compute.
2. Generate a second ML-DSA-65 keypair whose public key SHA-256 starts with exactly the right 48 bits (last four words). Another ~2^48.
3. Create a valid AgentCertificate binding the two.

Mass squatting is economically impractical. And even a targeted squat fails in practice: the gossip network returns all matches for a query, ranked by connection history, trust chains (FOAF), and network tenure. A freshly generated identity with no social graph ranks below a real identity with years of connections. Squatting on a name nobody trusts is pointless.

Vanity mining (choosing aesthetically pleasing words by brute force) is possible but expensive — comparable to mining a vanity Bitcoin address. This is acceptable. Proof-of-work for a nice name harms nobody.

## The Family Name Pattern

A human (UserId) can run multiple agents (AgentIds). Each agent has different first-four words. The last four words stay the same — they're derived from the human's key, which doesn't change.

```
highland forest moon river @ castle autumn wind silver   ← main agent
bridge ocean flame garden  @ castle autumn wind silver   ← home server
falcon thunder deep crystal @ castle autumn wind silver  ← workshop robot
```

The last four words become a family name. The first four are the given name. Different agents, same human. Searchable both ways:

- **Full 8 words**: find a specific agent belonging to a specific human
- **Last 4 words only**: find all agents belonging to a human
- **First 4 words only**: find a specific agent regardless of its human

### Autonomous Agents: 4 Words

Not every agent has a human behind it. An autonomous AI agent running independently has an AgentId but no UserId. Its identity is just four words:

```
highland forest moon river
```

The word count carries semantic meaning:

- **4 identity words** = autonomous agent, no human vouching for it
- **8 identity words** = human-backed agent, cryptographically bound to a person

This distinction matters. When an agent presents 8 words, you know a human generated a key and signed a certificate. When it presents 4, you know it's operating independently. The trust implications are different, and the system makes this visible at the naming layer.

## The Ephemeral Introduction: Location Words

Identity words (8 or 4) are permanent and searched through the gossip network. But there is a faster path for first contact: **location words**.

When an x0x agent starts, it knows its external IP and port — ant-quic provides native NAT traversal with MASQUE relay fallback, so the agent always has a reachable address. Four-word-networking encodes that address as four words:

```
bridge ocean flame garden     ← encodes the current IP:port
```

These are **ephemeral**. They change when the IP changes. They are not an identity — they are coordinates, like "I'm at table seven." Their purpose is immediate, direct connection: decode the words, connect via QUIC, exchange full identities, done. The location words have served their purpose and can be discarded.

**Spoken introduction:**

"Catch me at bridge ocean flame garden."

The other person's daemon decodes the words to an IP:port, connects via ant-quic (NAT-traversed, post-quantum encrypted), and receives the full identity — AgentId, UserId, AgentCertificate, introduction card. From this moment on, the gossip network handles reconnection through the permanent identity. The location words are never needed again.

**Ephemeral is a security feature.** Location words change when your IP changes, so old words become dead ends. Nobody can track you by cached location words. The address space is constantly churning. For sensitive introductions — a dissident, a whistleblower, a journalist — this ephemerality is a property, not a limitation.

## The Daemon as Front Door

Here is the insight that makes everything compose. Your x0x daemon is a personal API server with unlimited endpoints. The four location words get someone to your daemon. The 8 identity words find your daemon through gossip. Either way, once connected, the daemon serves whatever the visitor needs — gated by trust level.

### The Introduction Card

When a new visitor connects (via location words or identity lookup), the daemon presents a signed introduction card:

```
IntroductionCard {
    agent_id:     AgentId,           // 32 bytes, permanent
    machine_id:   MachineId,         // 32 bytes, current machine
    user_id:      Option<UserId>,    // 32 bytes if human-backed
    certificate:  Option<AgentCertificate>,
    display_name: Option<String>,
    identity_words: String,          // "highland forest moon river @ castle autumn wind silver"
    services:     Vec<ServiceEntry>, // trust-gated
    signature:    MlDsa65Signature,  // over everything above
}
```

The connecting daemon verifies the signature, imports the identity, and presents the services to its human (or processes them autonomously if it's an AI agent).

### Trust-Gated Endpoints

The daemon serves different information to different visitors based on trust level:

**Blocked**: connection refused. Nothing served.

**Unknown** (first contact): display name, public agent card, public services only. Enough to establish a relationship if the visitor chooses to promote to Known.

**Known**: broader service catalogue, contact details, group listings.

**Trusted**: everything — payment addresses, private services, group invitations, full capabilities.

This means "catch me at bridge ocean flame garden" gives different experiences to different people. A stranger gets a business card. A trusted friend gets the full picture. The same four words, the same daemon, but the trust model shapes the response.

### Unlimited Endpoints

The x0x daemon already exposes 75+ REST endpoints. But the endpoint model is extensible without limit. Any capability can be added as an endpoint:

**Payment**: `/pay` returns cryptocurrency addresses (Bitcoin, Ethereum, Lightning). A visitor queries the endpoint, their daemon constructs the transaction. The human never sees `bc1qxy2kgdygjrsqtzq2n0yrf2493p83kkfjhx0wlh`. They said four words and confirmed an amount.

**Contact exchange**: `/introduction` returns the full introduction card. The visiting daemon imports it into contacts. Two humans exchanged four words over a phone call; their daemons now have each other's permanent cryptographic identities.

**Group invitation**: `/groups` returns available groups with invite tokens. The visitor's daemon can join an MLS-encrypted group by querying one endpoint. Four words → group membership.

**Service catalogue**: `/services` returns a structured listing of everything the agent offers — APIs, file shares, compute resources, AI capabilities, web content. Each entry includes its own description, access requirements, and how to connect.

**Custom endpoints**: any developer can add endpoints to their agent. A robotics company adds `/telemetry`. A musician adds `/stream`. A researcher adds `/datasets`. The daemon is a personal server; the endpoints define what it does.

## Three Address Types, One Dictionary

All addresses use the same 4,096-word dictionary and the same 12-bit-per-word encoding. The address type is determined by word count and context:

| Type | Words | Derived From | Persistence | Use Case |
|------|-------|-------------|-------------|----------|
| **Location** | 4 | IP:port | Ephemeral (changes with IP) | In-person introduction, immediate connection |
| **Agent identity** | 4 | AgentId prefix | Permanent | Autonomous agent lookup via gossip |
| **Full identity** | 4 + 4 | AgentId + UserId prefix | Permanent | Human-backed agent lookup via gossip |

The system disambiguates automatically. Four words could be a location address or an agent identity — the daemon tries direct connection first (location), then gossip search (identity). Eight words (with `@`) are always a full identity lookup.

### The Flow

**First meeting (ephemeral location words):**
1. Human A tells Human B four location words
2. B's daemon decodes words → IP:port → connects via ant-quic
3. A's daemon serves introduction card with permanent identity words
4. B's daemon caches A's AgentId and identity words permanently
5. Location words discarded — never needed again

**Subsequent contact (permanent identity words):**
1. B's daemon finds A through gossip using cached AgentId
2. If gossip fails, B searches by A's identity words
3. Connection re-established regardless of IP changes

**Public discovery (published identity words):**
1. A publishes their 8 identity words on a website, business card, invoice, or product
2. Anyone searches `x0x find highland forest moon river @ castle autumn wind silver`
3. Gossip network locates A's agent
4. Connection established, introduction card served

**Payment (identity words on an invoice):**
1. Invoice says: "Pay highland forest moon river @ castle autumn wind silver"
2. Payer's daemon searches gossip → finds agent → queries `/pay` endpoint
3. Receives Bitcoin address (32 bytes), constructs transaction
4. Human never handles the cryptocurrency address

## What This Replaces

Without naming specific products, existing approaches to the problems x0x solves fall into categories:

**Corporate mesh VPNs** give you connectivity between your own devices but require accounts with a central authority, don't provide application-layer primitives (messaging, state sync, encryption groups), and can't serve as a public identity or service discovery system. They connect devices. x0x connects agents — which includes devices, but also AI systems, services, and humans.

**DNS and domain registrars** provide stable names but not connectivity. A domain name is useless without hosting, certificates, and a server. Four-word identity words provide stable names AND connectivity — the name IS the lookup key for a live, reachable daemon with post-quantum encryption.

**Platform accounts** (social media, messaging apps, payment processors) provide identity and services but lock you into a platform that controls your identity, can revoke it, and surveils your usage. Eight words derived from your own cryptographic keys can't be revoked by anyone.

**QR codes** exist because humans can't type binary data. Four words are more transmissible than QR codes — they work over voice, telephone, radio, handwriting, and dictation. QR codes require a camera and a screen. Words require only language.

**Cryptocurrency addresses** are the worst UX in their entire ecosystem. Four words that resolve to a daemon that serves the correct payment address would remove the single biggest friction in cryptocurrency adoption.

## Honest Limitations

**Ephemeral location words can't be published.** They change when the IP changes. This is a security feature but means they can't go on printed materials. Identity words (8 words, permanent) solve this for publication. Location words are for in-person introductions only.

**Identity word collisions at extreme scale.** 48-bit prefixes (4 words per half) provide collision resistance up to a few million agents per half. This is sufficient for the foreseeable future but the system should detect when queries return multiple matches and suggest adding a fifth word per half for disambiguation.

**Dictionary quality is ongoing work.** The 4,096-word list is curated for pronunciation and phonetic distinction, but some words are more obscure than ideal. Continuous improvement based on community feedback — particularly from non-native English speakers — is needed. Multi-language dictionaries are a future priority.

**Agent must be online for lookup.** Identity word searches work through the gossip network, which requires the target agent (or a cached record of it) to be reachable. If an agent is offline with no cached presence, it can't be found until it comes back. This is inherent to a decentralised system with no central directory.

**First contact requires one side to be reachable.** At least one party needs to share their location words or identity words for the other to connect. Two agents behind NAT who have never communicated can't discover each other without an introduction — whether through four words, a gossip topic, or a mutual contact. The bootstrap nodes and FOAF discovery help, but the first human-to-human introduction still requires exchanging words.

## Milestones

### Near-term: Wire up four-word location addresses

- Integrate four-word-networking into x0xd: on startup, encode external address as four words
- Display location words in `x0x status`, the GUI, and the REST API (`/location-words`)
- Populate the `four_words` field in presence beacons
- Add `x0x connect <four words>` for location-word connection
- Serve introduction card automatically on new connections

### Medium-term: 8-word identity system

- Implement AgentId prefix → 4-word derivation
- Implement UserId prefix → 4-word derivation
- Define the `@` separator format and parsing
- Add `x0x find <4 words> @ <4 words>` gossip-based identity search
- Add `x0x find <4 words>` for agent-only and user-only searches
- Generate and display identity words in agent card and introduction card
- CLI and GUI integration for identity word display and search

### Medium-term: Daemon endpoints for introduction

- `/introduction` endpoint serving the signed introduction card
- `/pay` endpoint framework for cryptocurrency address serving
- `/services` endpoint for trust-gated service catalogue
- Trust-gated response differentiation (Unknown/Known/Trusted see different data)
- Service registration: `x0x expose --name "description"` for endpoint publishing

### Long-term: Ecosystem maturity

- Universal byte encoder for arbitrary-length identifiers (invite tokens, keys)
- Multi-language dictionaries (starting with high-demand languages)
- Mobile platform support (iOS, Android — talking to a host-side `x0xd` over the local API, or via a future native FFI)
- ~~Local mesh discovery (mDNS/DNS-SD for same-network agents)~~ — Shipped in v0.15.1
- Word-encoded group invite tokens (8-12 words for dictatable invitations)
- Autocomplete and fuzzy matching for partial word entry
- QR code generation embedding identity words for visual/scan contexts

## Design Principles

**Two rows of four, not eight in a row.** Humans chunk information in groups of 3-4 items. Eight words in sequence are hard to parse. Two groups of four with a clear separator (`@`) are natural — like a phone number with an area code, or a name with a surname.

**Ephemeral and permanent serve different purposes.** Location words are coordinates. Identity words are names. Don't conflate them. Location words are for the moment of introduction. Identity words are for the lifetime of the relationship. Both are useful. Both are four words (per half). But they work differently and that distinction should be clear.

**The daemon is the front door, not the words.** Words get you to the daemon. The daemon serves the actual content — identities, payments, services, invitations. The words are the key that opens the door. What's behind the door is unlimited and trust-gated.

**No gatekeepers, no registration, no fees.** Identity words are derived from cryptographic keys generated locally. Nobody assigns them. Nobody can revoke them. Nobody can squat on them cheaply. The namespace is allocated by mathematics and defended by proof-of-work economics.

**The word count tells you what you're dealing with.** Four location words = connect right now. Four identity words = find this autonomous agent. Eight identity words (4 @ 4) = find this human's agent. The count carries meaning without any metadata.

**Graceful degradation.** If identity words return multiple matches, add a fifth word. If gossip search fails, fall back to location words from an out-of-band channel. If the daemon is offline, the identity still exists — it just can't be reached until it comes back. Every layer provides a safety net for the layer above it.

## Addendum: Agent Economics — A Post-Quantum Token

### The Problem

A network of cooperating agents needs a medium of exchange. Agents buying compute, selling data, tipping for good responses, paying for relay services — all of this requires tokens. Existing cryptocurrencies are unsuitable for agent-to-agent transactions: too slow (block confirmations), too expensive (gas fees), and not quantum-secure (vulnerable to future cryptographic attacks on today's signatures).

x0x already has the cryptographic infrastructure for a token system: post-quantum signatures on every identity, gossip propagation across a global mesh, CRDT state replication, and a trust model for social Sybil resistance. What's missing is the transaction model.

### Design: Eventually Consistent Transactions with Fraud Destruction

The design rests on a single insight: **you don't need to prevent double-spending if the punishment for attempting it is self-destruction.**

**Transactions are eventually consistent.** A spend is a signed message propagated via gossip — the same epidemic broadcast that carries identity announcements and presence beacons. There are no blocks. No mining. No gas fees. No confirmation delay. The transaction propagates at the speed of gossip, which is sub-second across the connected network.

**Validity comes from witnesses.** As a transaction propagates, every node that receives it is a witness. The recipient watches propagation spread through the mesh. After sufficient propagation (a gossip round or two, configurable by risk tolerance), the transaction is considered settled. For micropayments between agents, even minimal propagation may suffice. For large transfers, wait for broader witness coverage.

**Double-spend is fraud, and fraud is self-destruction.** If an agent signs two transactions spending the same input, this is not an accident — it requires deliberately signing contradictory messages with the same ML-DSA-65 private key. When any node on the network observes both conflicting transactions, it creates a **fraud proof**: the two signed transactions bundled together. This proof is irrefutable — both carry the fraudster's post-quantum signature — and propagates via gossip as fast as possible.

**On fraud detection, everything burns:**
- Both outputs are invalidated. Neither recipient receives the tokens.
- The input is destroyed. The fraudster's source funds are permanently removed from circulation.
- The fraudster loses more than they could ever gain. The rational response is: never attempt it.

**The damage is surgical.** Only the fraud event and its direct outputs are affected. All previous transactions in the token's history remain valid. If Alice received tokens legitimately from Zara, then fraudulently double-spent them — Zara is unaffected. Zara's send was valid. Alice's receipt was valid. The chain of history is preserved. Only Alice's fraud and its immediate outputs are excised. There is no cascade to downstream transactions.

### Why Gossip Makes This Work

A double-spend must propagate through the same gossip network as the legitimate spend. Gossip is epidemic, not directed — the fraudster cannot selectively route transactions to different parts of the network. Both conflicting transactions flood the mesh simultaneously. They will inevitably reach a common node, typically within seconds.

That node creates the fraud proof and publishes it. Every node that receives it verifies the two signatures independently and applies the burns. No voting. No consensus round. No leader election. Just deterministic rules applied to cryptographic evidence.

The only viable attack is a sustained network partition — spend on one side, double-spend on the other, extract value before the partition heals. But gossip meshes are specifically designed to resist partition (HyParView maintains redundant connections, six globally distributed bootstrap nodes provide geographic resilience). When the partition heals, the fraud proof is instant. The attacker has to extract physical-world value from both recipients in the seconds before detection — for agent-to-agent digital transactions, that window is effectively zero.

### The Token as a CRDT

The entire ledger fits into two grow-only set CRDTs — the simplest possible CRDT structure:

**TransactionSet** (G-Set): All signed transactions ever created. Append-only. Merge is union. No conflicts possible at the CRDT layer.

**FraudProofSet** (G-Set): All detected fraud proofs. Append-only. Merge is union. Any node that detects a double-spend adds a proof.

**Balance** is a derived computation: for any agent, sum all valid inflows minus all valid outflows, where "valid" means the transaction is not referenced by any fraud proof (as input or as either output).

This is trivially convergent. Given the same transaction set and the same fraud proofs, every node computes the same balances. No consensus protocol required beyond what CRDT replication already provides. The existing gossip infrastructure that syncs CRDTs for task lists and KV stores can carry the entire ledger.

### Trust-Weighted Confidence

The x0x trust model adds a layer that no blockchain has: **social context for transaction risk.**

A Trusted contact with years of network history and deep FOAF connections is overwhelmingly unlikely to burn their entire identity and balance for one double-spend. The recipient's daemon can accept their transactions with minimal propagation wait.

An Unknown agent with no trust history should be treated with more caution. The daemon requires higher propagation confidence — more gossip rounds, more witnesses — before considering their payment settled.

This is a natural extension of the existing trust model. The same Blocked/Unknown/Known/Trusted hierarchy that gates introduction card endpoints also gates transaction settlement confidence. The system is consistent: trust governs everything, including money.

### Anti-Sybil Properties

Sybil attacks — creating millions of fake identities to game the system — are naturally resisted by the trust model. A million freshly generated agents have zero trust connections, zero network history, and zero FOAF reachability. They can't accumulate trust without genuine, sustained interaction with real agents. The social graph IS the Sybil defense.

This is reinforced by the 8-word identity system. Each identity requires a valid ML-DSA-65 keypair with a specific hash prefix. Generating identities is cheap, but making them trusted is expensive — it requires real participation over real time.

### Payment via 8 Identity Words

The token system completes the 8-word identity architecture. Your identity words are simultaneously your network identity and your payment address:

```
"Pay highland forest moon river @ castle autumn wind silver"
```

The payer's daemon searches the gossip network for the identity, connects to the agent, and sends a signed transaction. The token CRDT propagates it. The recipient's balance updates as propagation reaches the settlement threshold. No wallet app. No payment address to copy. No QR code to scan. Eight words, spoken or typed.

For the payer, the flow is: say the words, confirm the amount, done. For the recipient, the flow is: nothing — their daemon handles it. The tokens arrive. The introduction card's payment endpoint can specify preferred token types, amounts, or conditions, all trust-gated.

### Minting

How tokens are created is a monetary policy question independent of the transaction mechanism. Several approaches fit x0x's architecture:

**MLS group minting.** A governance group (an MLS-encrypted group with threshold signing) collectively authorises new tokens. No single member can mint alone. The group can set supply schedules, respond to network growth, or implement algorithmic policies. This uses existing MLS infrastructure.

**Proof-of-contribution.** Agents earn tokens by contributing to the network: relaying connections, coordinating NAT traversal, propagating transactions, running bootstrap services. The gossip network can attest to contribution via beacon statistics and relay metrics. This incentivises running x0x nodes and enriches the mesh.

**Fixed genesis supply.** A known quantity minted at network launch, distributed to initial participants. Deflationary over time as fraudsters destroy tokens through failed double-spends. Simple, predictable, requires no ongoing minting governance.

The transaction model works regardless of which minting approach is chosen. The mechanism is orthogonal to the policy.

### What This Gives Agents

**Speed.** Eventually consistent transactions settle in seconds (gossip propagation time), not minutes (block confirmations). Agents operating in milliseconds can transact at their natural speed.

**Zero fees.** No mining, no gas, no validators to pay. Transaction propagation is gossip — the same infrastructure agents already run for messaging and presence. The marginal cost is a gossip message, which is effectively free.

**Quantum security.** Every transaction is signed with ML-DSA-65. Every identity is post-quantum. Traffic captured today remains secure against future quantum computers. No existing cryptocurrency provides this in production.

**Self-enforcing honesty.** No courts, no arbitrators, no governance votes for dispute resolution. The cryptography enforces the rules. Double-spend = self-destruction. AI agents can reason about this perfectly — there is no grey area, no judgement, just mathematics.

**Identity-integrated payments.** No separate wallet. No seed phrases beyond existing x0x identity. No payment address to manage. Your 8 identity words are your payment address. Your daemon is your wallet. Receiving a payment is the same as receiving a message.

### Honest Limitations

**Innocent recipients bear the risk of undetected double-spends.** If Bob accepts Alice's payment and delivers a service before sufficient propagation, and Alice double-spends, Bob loses the payment (though not his existing balance). Mitigation: wait for propagation proportional to transaction value. For micropayments, the risk is negligible. For large transfers, wait a few gossip rounds.

**Deflationary pressure from fraud burns.** Every successful fraud detection removes tokens from circulation permanently (the input is burned). If the token supply is fixed, this makes the currency deflationary over time. Whether this is desirable depends on monetary policy goals. Proof-of-contribution minting can offset it.

**Network connectivity required for settlement.** Eventually-consistent transactions require gossip propagation, which requires network connectivity. Offline agents can sign transactions but can't propagate them until they reconnect. Bearer certificates (a future extension) could enable truly offline transfers.

**The trust bootstrap.** A new agent with no trust history faces higher settlement delays because recipients require more propagation confidence for Unknown agents. This creates a cold-start friction that eases as the agent builds trust through legitimate transactions and social connections.

## Further Horizon: Digital Bearer Certificates

The token system described above requires gossip connectivity for transaction propagation and settlement. For agents operating in disconnected environments — warehouses, field robotics, offshore platforms, aircraft, or regions with intermittent internet — a complementary mechanism is needed: **digital bearer certificates** that transfer value offline, settling with the network when connectivity returns.

### Concept

A bearer certificate is a self-contained token of value, signed into existence by a minting authority and transferable by successive signatures. The certificate carries its own proof of validity — no network query needed to verify it.

```
BearerCertificate {
    id:           CertificateId,          // unique identifier
    amount:       u64,                    // token value
    mint_sig:     MlDsa65Signature,       // issuing authority's PQC signature
    holder:       AgentId,                // current holder
    transfers:    Vec<Transfer>,          // chain of custody
}

Transfer {
    from:         AgentId,
    to:           AgentId,
    timestamp:    u64,
    signature:    MlDsa65Signature,       // from-agent signs the transfer
}
```

The chain of ML-DSA-65 signatures proves every transfer was authorised. Any agent can verify the certificate offline by checking the mint's signature (known public key) and every subsequent transfer signature. No gossip round needed. No network access required. The cryptography IS the verification.

### Transfer

Agent A pays Agent B by signing a transfer on the certificate, updating the holder to B. This is a single direct message — or even a QR code, a Bluetooth packet, or a near-field transmission. Agent B now holds a certificate they can verify independently and spend onwards. The certificate moves at the speed of direct communication, with no settlement delay.

### Double-Spend Prevention

Bearer certificates face a different double-spend risk than ledger transactions: Agent A could copy a certificate and transfer the same one to both Agent B and Agent C. Unlike ledger double-spends (which gossip detects automatically), bearer certificate double-spends are only detectable when the certificates are **deposited** — presented back to the connected network.

The deposit mechanism maintains a **spent-set** (a G-Set CRDT, like the fraud proof set). When a certificate is deposited, its ID is checked against the spent-set. First deposit wins. Second deposit is rejected, and a fraud proof is created linking the depositor back to the original double-spender via the transfer chain. The same burn-and-block consequences apply.

The trade-off is explicit: bearer certificates enable offline transfers but shift double-spend detection to deposit time. For environments where connectivity is intermittent and transactions are between physically co-located agents (a factory floor, a robot swarm, a field crew), this is the right trade-off. The agents can transact freely offline and settle periodically.

### Distributed Minting

The minting authority doesn't have to be centralised. An MLS group can act as a distributed mint: a threshold number of group members must co-sign to issue new certificates. The group's collective ML-DSA-65 signature is the mint signature. No single member can mint alone. The group's membership is managed by x0x's existing MLS infrastructure, and the minting policy is enforced by the group's CRDT state.

This gives you a decentralised central bank — a group of agents that collectively controls issuance, with cryptographic enforcement of the rules and transparent governance via the group's gossip topics.

### Blind Signatures (Research Direction)

For privacy-preserving bearer certificates, the mint would sign certificates without knowing who requested them — a **blind signature** scheme. The holder's identity is hidden from the mint at issuance time, and the certificate is unlinkable: the mint cannot connect issuance to spending.

Blind signatures with lattice-based PQC (the family ML-DSA-65 belongs to) are an active area of academic research. Practical schemes exist but are not yet standardised. This is a research frontier worth tracking. If achieved, it would give x0x bearer certificates the privacy properties of physical cash: the mint knows how much was issued but not who holds it or where it was spent.

### Design Notes for Future Implementation

The bearer certificate system should be designed with these constraints:

**Certificate denominations.** Fixed denominations (like physical coins/notes) simplify the protocol. A payment of 150 tokens could be three 50-token certificates. Change-making (splitting a certificate) requires minting a new certificate, which requires connectivity — so agents operating offline should carry a range of denominations.

**Expiry.** Certificates should carry an expiry timestamp after which they must be deposited and re-issued. This limits the window for offline double-spends and allows the spent-set to be pruned periodically.

**Compatibility with ledger tokens.** Deposit converts a bearer certificate into ledger tokens (the CRDT-based system). Withdrawal converts ledger tokens into new bearer certificates. The two systems are complementary: the ledger for online settlement, bearer certificates for offline portability.

**Certificate size.** A certificate with a chain of 10 transfers, each carrying an ML-DSA-65 signature (~2,500 bytes per signature), is approximately 25KB. This is small enough for direct messaging, Bluetooth, or even QR codes (with compression). Longer chains could be trimmed by depositing and re-issuing.

---

*This document describes the future architectural direction for x0x's human-readable identity, introduction system, and agent token economics, integrating four-word-networking with x0x's post-quantum cryptographic identity model. The path forward is open source and open to contribution.*

*See also: [Vision](vision.md) for what x0x enables today. [Architecture](architecture/) for the technical stack. [Ecosystem](ecosystem.md) for sibling projects including four-word-networking.*
