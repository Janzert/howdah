//! Invitations (challenges) to play a particular player.
//!
//! ASIP has no invitation actions, so these follow the browser lobby
//! (checked 2026-10-06): `invite.cgi` (a form) sends one and redirects to
//! the inviter's waiting page; `inviteWait.cgi` is a long poll (held up to
//! ~110 s) that says whether it was accepted (with the new game's id),
//! declined (with the reason) or not answered yet; the invited player
//! accepts with `acceptInvite.cgi` (a GET) and declines with a form to
//! `declineInvite.cgi`; the inviter cancels with a form to
//! `cancelInvite.cgi`. The lobby's `state` lists open invitations both
//! ways (`invitedmegames`, `iinvitedgames`).
//!
//! Accepting creates the game with both players assigned; neither is
//! seated. Such a game can't be cancelled (`cancelopengame` answers `ok`
//! and does nothing): it has to be played or resigned.

use howdah_arimaa::Color;

use crate::state::side_from_letter;
use crate::wire::Record;

/// An open invitation, from the lobby's lists.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Invitation {
    /// The inviting and invited players' ids. With `created`, they name
    /// the invitation in the accept, decline and cancel requests.
    pub inviter_id: String,
    pub invited_id: String,
    /// When it was sent (`createdts`, Unix seconds).
    pub created: String,
    /// The other player (the list's `opponent`): their username and
    /// rating.
    pub opponent: Option<String>,
    pub opponent_rating: Option<String>,
    /// The side the inviter plays.
    pub inviter_side: Color,
    pub time_control: Option<String>,
    pub rated: bool,
    /// The inviter's message (`reason`).
    pub message: Option<String>,
}

impl Invitation {
    pub fn from_record(r: &Record) -> Option<Invitation> {
        let opponent = r.object("opponent");
        Some(Invitation {
            inviter_id: r.nonempty("invitor")?,
            invited_id: r.nonempty("invited")?,
            created: r.nonempty("createdts")?,
            opponent: opponent.as_ref().and_then(|o| o.nonempty("username")),
            opponent_rating: opponent.as_ref().and_then(|o| o.nonempty("rating")),
            inviter_side: side_from_letter(&r.nonempty("invitorside")?)?,
            time_control: r.nonempty("timecontrol"),
            rated: r.flag("rated"),
            message: r.nonempty("reason"),
        })
    }
}

/// What the inviter's waiting page (`inviteWait.cgi`) said.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InviteOutcome {
    /// Accepted: the game's gameroom id.
    Accepted { gid: String },
    /// Declined, with the page's words ("Game declined by …", and the
    /// reason if one was given).
    Declined { message: String },
    /// No answer before the page gave up waiting.
    NoResponse,
    /// Something else (the page's text).
    Other(String),
}

/// Reads the waiting page.
pub fn parse_wait(page: &str) -> InviteOutcome {
    const GAME: &str = "opengamewin.cgi?gameid=";
    if let Some(i) = page.find(GAME) {
        let gid: String = page[i + GAME.len()..].chars().take_while(char::is_ascii_digit).collect();
        if !gid.is_empty() {
            return InviteOutcome::Accepted { gid };
        }
    }
    let text = text_of(page);
    if let Some(i) = text.find("declined by") {
        let message = text[i..].trim_end_matches("Close").trim();
        // "Game declined by X. Reason" reads better capitalised.
        return InviteOutcome::Declined { message: format!("D{}", &message[1..]) };
    }
    if text.contains("No Response") {
        return InviteOutcome::NoResponse;
    }
    InviteOutcome::Other(text.chars().take(200).collect())
}

/// HTML's text without tags or scripts, whitespace collapsed.
fn text_of(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    text.replace("&nbsp;", " ").split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invitations_are_read() {
        // As `state` listed one on 2026-10-06 (names changed).
        let r = Record::decode(
            r#"{"invitedmegames":[{"invitor":"21487","status":"i","createdts":"1791310268",
                "reason":"A game?","opponent":{"rating":"1400","username":"inviter","id":"21487","type":"h"},
                "invitorside":"w","timecontrol":"2m/5m/100/0/30m","invited":"19377","rated":"0"}]}"#,
        )
        .unwrap();
        let inv = Invitation::from_record(&r.list("invitedmegames")[0]).unwrap();
        assert_eq!(
            inv,
            Invitation {
                inviter_id: "21487".into(),
                invited_id: "19377".into(),
                created: "1791310268".into(),
                opponent: Some("inviter".into()),
                opponent_rating: Some("1400".into()),
                inviter_side: Color::Gold,
                time_control: Some("2m/5m/100/0/30m".into()),
                rated: false,
                message: Some("A game?".into()),
            }
        );
    }

    #[test]
    fn waiting_pages_are_read() {
        let accepted = r#"<meta HTTP-EQUIV="REFRESH" CONTENT="0; URL=opengamewin.cgi?gameid=539505&side=w">
Invite accepted."#;
        assert_eq!(parse_wait(accepted), InviteOutcome::Accepted { gid: "539505".into() });
        let declined = r#"<h2 align=center>Game Declined</h2>
    <blockquote><p align=center><b><center>
  <p>Game declined by someone.
<p><i>Not now, thanks</i>
<p><a href="javascript:window.close()">Close</a>
</center></b></p></blockquote>"#;
        assert_eq!(
            parse_wait(declined),
            InviteOutcome::Declined { message: "Declined by someone. Not now, thanks".into() }
        );
        let open = r#"<h2 align=center>No Response</h2><blockquote><p align=center><b>Check the
'Pending Invites' or 'My Games' section to see if invite was accepted. <br> </b></p></blockquote>"#;
        assert_eq!(parse_wait(open), InviteOutcome::NoResponse);
    }
}
