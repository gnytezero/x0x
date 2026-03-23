# Identity Architecture

x0x uses a three-layer identity model where each layer builds on the one below it.

## Layer 0: Machine Identity

A `MachineId` is derived from an ML-DSA-65 public key:

```
MachineId = SHA-256(ML-DSA-65 public key bytes)
```

The key pair is stored in `~/.x0x/machine.key` (bincode format). It is auto-generated on first run and never leaves the machine. The `MachineId` is used as the QUIC transport identity — the same key pair is passed to `ant-quic::NodeConfig` so that the QUIC `PeerId` equals the `MachineId`.

**Purpose**: Hardware-pinned identity for NAT traversal and transport authentication.

## Layer 1: Agent Identity

An `AgentId` is derived from a separate ML-DSA-65 key pair:

```
AgentId = SHA-256(ML-DSA-65 public key bytes)
```

Stored in `~/.x0x/agent.key`. Portable — can be copied to another machine to run the same logical agent on different hardware.

**Purpose**: Persistent agent identity that survives hardware changes.

## Layer 2: User Identity (optional)

A `UserId` binds a human identity to an agent via an `AgentCertificate`:

```
AgentCertificate = sign(UserKeypair, (AgentId, UserId))
```

Never auto-generated. Opt-in only (`with_user_key()`). When included in an announcement, both the certificate and user ID are present or neither is.

**Purpose**: Optional human accountability layer.

## Identity Unification

Before Milestone 1, x0x generated separate key pairs for transport (ant-quic) and identity (x0x). After unification, they share the same ML-DSA-65 key pair:

```
machine.key → MachineKeypair → ML-DSA-65 key pair
                              ├── MachineId = SHA-256(public key)
                              └── ant-quic PeerId = SHA-256(public key)
```

This means `agent.machine_id() == ant-quic PeerId` — verified by `identity_unification_test.rs`.

## Identity Announcements

An `IdentityAnnouncement` is broadcast by agents when they join or heartbeat. It carries:

| Field | Purpose |
|-------|---------|
| `agent_id` | Portable agent identity |
| `machine_id` | Hardware identity (= QUIC PeerId) |
| `user_id` | Optional human identity |
| `machine_public_key` | Full ML-DSA-65 public key bytes (for signature verification) |
| `machine_signature` | ML-DSA-65 signature over all unsigned fields |
| `agent_certificate` | Optional user→agent binding certificate |
| `addresses` | Reachability hints |
| `announced_at` | Unix timestamp |
| `nat_type` | NAT classification from network layer |
| `can_receive_direct` | Whether direct inbound connections work |
| `is_relay` | Whether node is relaying for others |
| `is_coordinator` | Whether node is coordinating NAT timing |

The announcement is signed by the machine key to bind the portable agent identity to this specific machine. Verification:

1. Parse `machine_public_key` as ML-DSA-65 public key
2. Derive `machine_id = SHA-256(machine_public_key)` and check it matches `announcement.machine_id`
3. Verify `machine_signature` over the serialized unsigned fields
4. If `user_id` is present, verify `agent_certificate` and check its `agent_id` and `user_id` match

## Trust Evaluation

The identity listener applies `TrustEvaluator` to every incoming announcement:

```
TrustDecision = evaluate((agent_id, machine_id), ContactStore)
```

Decision flow:
1. Agent not in store → `Unknown` (cache but don't trust)
2. `TrustLevel::Blocked` → `RejectBlocked` (drop, don't cache)
3. `IdentityType::Pinned` + machine not in pinned list → `RejectMachineMismatch` (drop)
4. `IdentityType::Pinned` + machine in pinned list → `Accept`
5. `TrustLevel::Trusted` → `Accept`
6. `TrustLevel::Known` → `AcceptWithFlag`
7. `TrustLevel::Unknown` → cache with unknown tag

Rejected announcements (`RejectBlocked`, `RejectMachineMismatch`) are silently dropped and not added to the discovery cache.

## Key Storage

All key files use bincode 1.x format:

```
~/.x0x/
  machine.key   # MachineKeypair: {public_key: [u8], secret_key: [u8]}
  agent.key     # AgentKeypair:   {public_key: [u8], secret_key: [u8]}
  user.key      # UserKeypair:    {public_key: [u8], secret_key: [u8]}
  contacts.json # ContactStore:   JSON with contacts array
```

The contacts file uses JSON (not bincode) for human readability and editability.
