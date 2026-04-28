//! Research-stage features — not yet production-ready.
//!
//! Modules here have known unimplemented dependencies.  They are gated behind
//! Labs flags and must not be reachable from stable code paths.
//!
//! | Module  | Blocker                                               |
//! |---------|-------------------------------------------------------|
//! | pricing | Needs P2P gossip layer + privacy-preserving transport |

pub mod pricing;
