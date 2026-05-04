use std::fs;
use std::io;
use std::path::PathBuf;

use domain::{Scope, ScopeError, ScopeReader, ScopeWriter, SessionId};

pub struct FileScopeStore {
    dir: PathBuf,
}

impl FileScopeStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn path_for(&self, id: &SessionId) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }
}

impl ScopeReader for FileScopeStore {
    fn scope_for(&self, session_id: &SessionId) -> Result<Option<Scope>, ScopeError> {
        match fs::read_to_string(self.path_for(session_id)) {
            Ok(content) => {
                let inner: String = serde_json::from_str(&content)
                    .map_err(|e| ScopeError::Unavailable(Box::new(e)))?;
                Ok(Some(Scope::new(inner)))
            }
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(ScopeError::Unavailable(Box::new(e))),
        }
    }
}

impl ScopeWriter for FileScopeStore {
    fn assign(&self, session_id: &SessionId, scope: Scope) -> Result<(), ScopeError> {
        fs::create_dir_all(&self.dir).map_err(|e| ScopeError::Unavailable(Box::new(e)))?;
        let inner: String = scope.into_inner();
        let json =
            serde_json::to_string(&inner).map_err(|e| ScopeError::Unavailable(Box::new(e)))?;
        fs::write(self.path_for(session_id), json)
            .map_err(|e| ScopeError::Unavailable(Box::new(e)))?;
        Ok(())
    }
}
