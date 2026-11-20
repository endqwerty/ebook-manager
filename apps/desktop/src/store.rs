//! The write thread: the one owner of the `Library`. Every import, edit and
//! saved position is a job sent to it. Showing books never goes through it,
//! apart from asking once for the read-only connection.
use anyhow::{Result, anyhow};
use ebook_library::{Library, ReadingPosition};
use std::{path::PathBuf, sync::mpsc};

type Job = Box<dyn FnOnce(&mut Result<Library>) + Send>;
#[derive(Clone)]
pub struct Store(mpsc::Sender<Job>);

impl Store {
    /// `finished` runs on the write thread after every job. A job may have
    /// changed the catalogue, and the listener asks the catalogue whether
    /// it did.
    pub fn open(path: PathBuf, finished: impl Fn() + Send + 'static) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            let mut library = Library::open(&path);
            for job in rx {
                if library.is_err() {
                    library = Library::open(&path);
                }
                job(&mut library);
                finished();
            }
        });
        Self(tx)
    }

    pub async fn save_position(&self, id: String, position: ReadingPosition) -> Result<()> {
        self.run(move |lib| lib.save_position(&id, &position)).await
    }

    fn submit<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Library) -> Result<T> + Send + 'static,
    ) -> Result<async_channel::Receiver<Result<T>>> {
        let (tx, rx) = async_channel::bounded(1);
        self.0
            .send(Box::new(move |lib| {
                let result = lib.as_mut().map_err(|e| anyhow!("{e:#}")).and_then(f);
                let _ = tx.send_blocking(result);
            }))
            .map_err(|_| anyhow!("Library closed"))?;
        Ok(rx)
    }

    /// For the GTK thread.
    pub async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Library) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.submit(f)?.recv().await?
    }

    /// For a worker thread, which may wait.
    pub fn call<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Library) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        self.submit(f)?.recv_blocking()?
    }
}
