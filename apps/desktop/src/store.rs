use anyhow::{Result, anyhow};
use ebook_library::{Library, ReadingPosition};
use std::{path::PathBuf, sync::mpsc};

type Job = Box<dyn FnOnce(&mut Result<Library>) + Send>;
#[derive(Clone)]
pub struct Store(mpsc::Sender<Job>);

impl Store {
    pub fn open(path: PathBuf) -> Self {
        let (tx, rx) = mpsc::channel::<Job>();
        std::thread::spawn(move || {
            let mut library = Library::open(&path);
            for job in rx {
                if library.is_err() {
                    library = Library::open(&path);
                }
                job(&mut library);
            }
        });
        Self(tx)
    }

    pub async fn save_position(&self, id: String, position: ReadingPosition) -> Result<()> {
        self.run(move |lib| lib.save_position(&id, &position)).await
    }

    pub async fn run<T: Send + 'static>(
        &self,
        f: impl FnOnce(&mut Library) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let (tx, rx) = async_channel::bounded(1);
        self.0
            .send(Box::new(move |lib| {
                let result = lib.as_mut().map_err(|e| anyhow!("{e:#}")).and_then(f);
                let _ = tx.send_blocking(result);
            }))
            .map_err(|_| anyhow!("Library closed"))?;
        rx.recv().await?
    }
}
