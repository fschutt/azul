//! Who is who on the network and who may do what: the names that keep the protocol from being phone-only.
//!
//! A NODE is one running AzReview (it has an identity key); a PEER is any key that talks to it: today the user's phone,
//! later other AzReviews of the user's own machines and of colleagues. What the Mac keeps of each peer it has paired is a
//! [`TrustRecord`]: the key, a name to show, whether it is a device or a person, and which of the two groups it is in. A
//! SCREEN is one window of AzReview with a [`Role`], placed on a display of some node; frames that act on a screen carry its
//! [`ScreenId`]. Who may command which screen is [`may_command`], a pure function, so every transport and every kind of
//! peer is held to the same rule.
//!
//! Nothing here shares a session, draws a screen map or claims a screen: these are the types and the rule, kept so that the
//! phone does not box the protocol in.

use crate::frame::Role;

/// A screen of AzReview that a frame is about. 0 is the main review screen, the only one the phone knows today.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ScreenId(pub u32);

impl ScreenId {
    /// The main review screen of a node.
    pub const MAIN: ScreenId = ScreenId(0);
}

impl core::fmt::Display for ScreenId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "screen {}", self.0)
    }
}

/// What kind of thing a peer is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PeerKind {
    /// A device of the user's: the phone, another computer.
    Device,
    /// Another person's AzReview.
    Person,
}

/// The group of the Mac's trust a peer is in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrustGroup {
    /// The user's own devices: they control each other freely, every screen at once.
    MyDevices,
    /// People who joined a session: they see what is shared, but every screen stays owned by its node.
    Collaborator,
}

/// What the Mac keeps of a peer it paired (the Mac side keeps it; the phone keeps only where to find its Mac).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrustRecord {
    /// The peer's key, as the transport authenticated it.
    pub peer: String,
    /// A name to show.
    pub name: String,
    pub kind: PeerKind,
    pub group: TrustGroup,
}

impl TrustRecord {
    /// A phone that was just paired: a device of the user's own group.
    #[must_use]
    pub fn phone(peer: &str, name: &str) -> TrustRecord {
        TrustRecord { peer: peer.to_string(), name: name.to_string(), kind: PeerKind::Device, group: TrustGroup::MyDevices }
    }
}

/// What a node knows of one of its screens.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenInfo {
    pub id: ScreenId,
    /// The node the screen belongs to.
    pub owner: String,
    /// The peer that drives the screen now (the owner unless it was handed over).
    pub controller: String,
    pub role: Role,
}

/// Why a command was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Denied {
    /// The peer is not paired.
    NotTrusted,
    /// The peer is a collaborator and does not control that screen.
    NotController,
    /// There is no such screen.
    UnknownScreen,
    /// The screen belongs to another node: a command for it does not go through this one.
    NotOurScreen,
}

impl core::fmt::Display for Denied {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(match self {
            Denied::NotTrusted => "the peer is not paired",
            Denied::NotController => "the peer does not control that screen",
            Denied::UnknownScreen => "there is no such screen",
            Denied::NotOurScreen => "that screen belongs to another node",
        })
    }
}

/// Whether the peer with `record` (`None`: not paired) may command `screen` (`None`: no such screen) on the node whose key is
/// `node`. The screen is looked at first (so that nothing tells which peers are paired), then whose it is, then the trust:
/// the node itself always may; the user's own devices may command every screen of the node; a collaborator only the screen it
/// controls.
///
/// # Errors
/// Why not: [`Denied`].
pub fn may_command(record: Option<&TrustRecord>, screen: Option<&ScreenInfo>, node: &str) -> Result<(), Denied> {
    let screen = screen.ok_or(Denied::UnknownScreen)?;
    if screen.owner != node {
        return Err(Denied::NotOurScreen);
    }
    let record = record.ok_or(Denied::NotTrusted)?;
    if record.peer == node {
        return Ok(());
    }
    match record.group {
        TrustGroup::MyDevices => Ok(()),
        TrustGroup::Collaborator if screen.controller == record.peer => Ok(()),
        TrustGroup::Collaborator => Err(Denied::NotController),
    }
}

#[cfg(test)]
#[path = "access_test.rs"]
mod access_test;
