//! arimaa.com's puzzles (`howdah_gameroom::Puzzles`): the list, fetched
//! once and kept, and each puzzle's answer when it's opened. A puzzle's
//! page (its hint and author) is fetched only when the hint is asked for,
//! so opening a puzzle costs the server one request.

use howdah_gameroom::{DEFAULT_PUZZLES, Http, PuzzleGroup, PuzzlePage, Puzzles};
use tokio::sync::Mutex;

use crate::dto::{ApiError, PuzzleEntryView, PuzzleGroupView};

pub struct PuzzleBook {
    puzzles: Puzzles,
    list: Mutex<Option<Vec<PuzzleGroup>>>,
}

fn net_error(e: howdah_gameroom::Error) -> ApiError {
    ApiError::state(format!("arimaa.com: {e}"))
}

impl PuzzleBook {
    pub fn new() -> PuzzleBook {
        let http = Http::new(&howdah_gameroom::user_agent(), None).expect("an HTTP client");
        PuzzleBook { puzzles: Puzzles::new(http, DEFAULT_PUZZLES), list: Mutex::new(None) }
    }

    /// The list, fetched the first time (or again with `refresh`).
    pub async fn list(&self, refresh: bool) -> Result<Vec<PuzzleGroupView>, ApiError> {
        let mut list = self.list.lock().await;
        if refresh || list.is_none() {
            *list = Some(self.puzzles.list().await.map_err(net_error)?);
        }
        Ok(list.as_ref().expect("fetched").iter().map(view).collect())
    }

    /// A puzzle's title in the list, if the list was fetched.
    pub async fn title(&self, id: &str) -> Option<String> {
        let list = self.list.lock().await;
        list.as_ref()?.iter().flat_map(|g| &g.puzzles).find(|p| p.id == id).map(|p| p.title.clone())
    }

    /// A puzzle's answer file (viewer variables).
    pub async fn answer(&self, id: &str) -> Result<String, ApiError> {
        self.puzzles.answer(id).await.map_err(net_error)
    }

    pub async fn page(&self, id: &str) -> Result<PuzzlePage, ApiError> {
        self.puzzles.page(id).await.map_err(net_error)
    }
}

fn view(g: &PuzzleGroup) -> PuzzleGroupView {
    PuzzleGroupView {
        name: g.name.clone(),
        puzzles: g
            .puzzles
            .iter()
            .map(|p| PuzzleEntryView { id: p.id.clone(), title: p.title.clone() })
            .collect(),
    }
}
