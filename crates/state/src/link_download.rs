//! Downloading from a link through the listener's own yt-dlp, one link at a time.

use std::path::{Path, PathBuf};

use gpui::{Context, Entity, SharedString, Task};
use music::ytdlp::{self, Progress, Request, Saved, Trouble, Update};
use tokio::sync::mpsc::unbounded_channel;
use tokio::sync::oneshot;

use crate::{AppSettings, Io, Outcome, Session, Toast, Toasts};

/// Where the download from a link stands.
#[derive(Clone, Debug, PartialEq)]
pub enum LinkState {
    Idle,
    /// yt-dlp is running. `name` is the file it is writing, once it has said so, and
    /// `progress` is the last it reported.
    Running {
        name: Option<SharedString>,
        progress: Progress,
    },
    /// The last download did not happen, and why.
    Failed(Trouble),
}

/// Runs yt-dlp for one link and brings what it saved into Local Music. It also knows where
/// yt-dlp is, which Settings shows and the dialog needs before it can start.
pub struct LinkDownload {
    settings: Entity<AppSettings>,
    session: Entity<Session>,
    io: Io,
    /// The yt-dlp found the last time it was looked for.
    program: Option<PathBuf>,
    state: LinkState,
    /// Stops the running download when sent on or dropped.
    cancel: Option<oneshot::Sender<()>>,
    task: Option<Task<()>>,
    looking: Option<Task<()>>,
}

impl LinkDownload {
    pub fn new(
        settings: Entity<AppSettings>,
        session: Entity<Session>,
        io: Io,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut this = Self {
            settings,
            session,
            io,
            program: None,
            state: LinkState::Idle,
            cancel: None,
            task: None,
            looking: None,
        };
        this.look(cx);
        this
    }

    /// The yt-dlp that would run, or `None` when there is none to run.
    pub fn program(&self) -> Option<&Path> {
        self.program.as_deref()
    }

    pub fn state(&self) -> &LinkState {
        &self.state
    }

    pub fn running(&self) -> bool {
        matches!(self.state, LinkState::Running { .. })
    }

    /// The folders a download can go to: the ones Local Music is read from, since that is where
    /// the scan that follows finds the file.
    pub fn folders(&self, cx: &gpui::App) -> Vec<PathBuf> {
        let paths = self.session.read(cx).local_paths();
        paths.into_iter().map(PathBuf::from).collect()
    }

    /// Looks for yt-dlp again, at the path in the settings and then on `PATH`. The listener may
    /// have installed it since the last look, so the dialog and the settings row ask each time
    /// they open. It reads the disk, so it runs off the main thread.
    pub fn look(&mut self, cx: &mut Context<Self>) {
        let chosen = self.settings.read(cx).ytdlp_path().map(Path::to_path_buf);
        let found = self
            .io
            .spawn_blocking(move || ytdlp::locate(chosen.as_deref()));
        self.looking = Some(cx.spawn(async move |this, cx| {
            let found = found.await.ok().flatten();
            this.update(cx, |this, cx| {
                if this.program != found {
                    this.program = found;
                    cx.notify();
                }
            })
            .ok();
        }));
    }

    /// Keeps the listener's choice of yt-dlp, or goes back to the one on `PATH` with `None`.
    pub fn choose_program(&mut self, path: Option<PathBuf>, cx: &mut Context<Self>) {
        self.settings
            .update(cx, |settings, cx| settings.set_ytdlp_path(path, cx));
        self.look(cx);
    }

    /// Starts downloading `link` into `folder`, one of [`Self::folders`]. A link that is not an
    /// http or https address, a missing yt-dlp and a folder that is not the library's each end
    /// as [`LinkState::Failed`] without running anything.
    pub fn start(&mut self, link: &str, folder: PathBuf, playlist: bool, cx: &mut Context<Self>) {
        if self.running() {
            return;
        }
        let request = match (self.program.clone(), ytdlp::link(link)) {
            (None, _) => Err(Trouble::Missing),
            (_, None) => Err(Trouble::Link),
            (Some(_), Some(_)) if !self.folders(cx).contains(&folder) => Err(Trouble::Folder),
            (Some(program), Some(link)) => Ok(Request {
                program,
                link,
                folder,
                playlist,
            }),
        };
        let request = match request {
            Ok(request) => request,
            Err(trouble) => {
                self.state = LinkState::Failed(trouble);
                cx.notify();
                return;
            }
        };

        let (cancel, cancelled) = oneshot::channel();
        let (updates, mut heard) = unbounded_channel();
        let report = move |update| {
            updates.send(update).ok();
        };
        let work = self.io.spawn(ytdlp::download(request, report, cancelled));
        self.cancel = Some(cancel);
        self.state = LinkState::Running {
            name: None,
            progress: Progress::default(),
        };
        cx.notify();
        self.task = Some(cx.spawn(async move |this, cx| {
            // The queue closes when the download ends, whichever way it ends.
            while let Some(update) = heard.recv().await {
                if this.update(cx, |this, cx| this.heard(update, cx)).is_err() {
                    return;
                }
            }
            let outcome = work
                .await
                .unwrap_or_else(|_| Err(Trouble::Failed(String::new())));
            this.update(cx, |this, cx| this.finish(outcome, cx)).ok();
        }));
    }

    /// Stops the running download. yt-dlp is ended and the files it had not finished are
    /// removed, after which the state goes back to idle.
    pub fn cancel(&mut self, cx: &mut Context<Self>) {
        if let Some(cancel) = self.cancel.take() {
            cancel.send(()).ok();
            cx.notify();
        }
    }

    /// Forgets a failure, once the listener has read it or tries again.
    pub fn dismiss(&mut self, cx: &mut Context<Self>) {
        if matches!(self.state, LinkState::Failed(_)) {
            self.state = LinkState::Idle;
            cx.notify();
        }
    }

    fn heard(&mut self, update: Update, cx: &mut Context<Self>) {
        let LinkState::Running { name, progress } = &mut self.state else {
            return;
        };
        match update {
            // A new file starts from nothing, whatever the last one had reached.
            Update::File(file) => {
                *name = Some(file.into());
                *progress = Progress::default();
            }
            Update::Progress(reported) => *progress = reported,
        }
        cx.notify();
    }

    /// Settles a download that ended. What it saved is announced and scanned into Local Music,
    /// a failure is kept for the dialog to explain, and a cancelled one leaves nothing to say.
    fn finish(&mut self, outcome: Result<Vec<Saved>, Trouble>, cx: &mut Context<Self>) {
        self.cancel = None;
        self.state = match outcome {
            Ok(saved) => {
                let name = saved.first().map(|first| first.title.clone());
                let mut toast = Toast::new(Outcome::Done, "toast-link-downloaded")
                    .name(name.unwrap_or_default())
                    .count(saved.len().saturating_sub(1));
                if saved.iter().any(|saved| !saved.tagged) {
                    toast = toast.note("toast-link-downloaded-untagged");
                }
                Toasts::post(toast, cx);
                self.session
                    .update(cx, |session, cx| session.rescan_local(false, cx));
                LinkState::Idle
            }
            Err(Trouble::Cancelled) => LinkState::Idle,
            Err(trouble) => LinkState::Failed(trouble),
        };
        cx.notify();
    }
}
