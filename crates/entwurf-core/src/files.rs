//! File boundary shared by native workspaces and a single browser worker.
//! Browser bytes are persisted transactionally by the host in IndexedDB.

#[cfg(not(feature = "browser"))]
pub use std::fs::*;
#[cfg(not(feature = "browser"))]
pub fn exists(path: impl AsRef<std::path::Path>) -> bool {
    path.as_ref().exists()
}

#[cfg(feature = "browser")]
pub use memory::*;

#[cfg(feature = "browser")]
mod memory {
    use std::{
        cell::RefCell,
        collections::{BTreeMap, BTreeSet},
        ffi::OsString,
        io::{self, Cursor, Read},
        path::{Component, Path, PathBuf},
    };

    #[derive(Default)]
    struct Tree {
        files: BTreeMap<PathBuf, Vec<u8>>,
        dirs: BTreeSet<PathBuf>,
    }
    thread_local! { static TREE: RefCell<Tree> = RefCell::new(Tree::default()); }

    fn invalid(message: &str) -> io::Error {
        io::Error::new(io::ErrorKind::InvalidInput, message)
    }
    fn missing() -> io::Error {
        io::Error::new(io::ErrorKind::NotFound, "virtual file does not exist")
    }
    fn normalize(path: &Path) -> io::Result<PathBuf> {
        let text = path.to_str().ok_or_else(|| invalid("path must be UTF-8"))?;
        if !text.starts_with('/') || text.contains(['\\', '\0']) {
            return Err(invalid("virtual path must be absolute POSIX"));
        }
        let mut out = PathBuf::from("/");
        for part in path.components() {
            match part {
                Component::RootDir | Component::CurDir => {}
                Component::Normal(v) => out.push(v),
                _ => return Err(invalid("parent or prefixed virtual paths are forbidden")),
            }
        }
        Ok(out)
    }
    pub fn reset() {
        TREE.with(|tree| *tree.borrow_mut() = Tree::default());
    }
    pub fn list_files() -> Vec<String> {
        TREE.with(|tree| {
            tree.borrow()
                .files
                .keys()
                .map(|p| p.to_string_lossy().into_owned())
                .collect()
        })
    }
    pub fn exists(path: impl AsRef<Path>) -> bool {
        let Ok(path) = normalize(path.as_ref()) else {
            return false;
        };
        TREE.with(|tree| {
            let tree = tree.borrow();
            path == Path::new("/") || tree.files.contains_key(&path) || tree.dirs.contains(&path)
        })
    }
    pub fn canonicalize(path: impl AsRef<Path>) -> io::Result<PathBuf> {
        let path = normalize(path.as_ref())?;
        if exists(&path) {
            Ok(path)
        } else {
            Err(missing())
        }
    }
    pub fn create_dir_all(path: impl AsRef<Path>) -> io::Result<()> {
        let path = normalize(path.as_ref())?;
        TREE.with(|tree| {
            let mut tree = tree.borrow_mut();
            if path.ancestors().any(|p| tree.files.contains_key(p)) {
                return Err(invalid("file conflicts with directory"));
            }
            for p in path.ancestors() {
                tree.dirs.insert(p.to_owned());
            }
            Ok(())
        })
    }
    pub fn read(path: impl AsRef<Path>) -> io::Result<Vec<u8>> {
        let path = normalize(path.as_ref())?;
        TREE.with(|tree| tree.borrow().files.get(&path).cloned().ok_or_else(missing))
    }
    pub fn write(path: impl AsRef<Path>, bytes: impl AsRef<[u8]>) -> io::Result<()> {
        let path = normalize(path.as_ref())?;
        create_dir_all(path.parent().ok_or_else(|| invalid("cannot write root"))?)?;
        TREE.with(|tree| {
            let mut tree = tree.borrow_mut();
            if tree.dirs.contains(&path) {
                return Err(invalid("directory conflicts with file"));
            }
            tree.files.insert(path, bytes.as_ref().to_vec());
            Ok(())
        })
    }
    pub fn remove_file(path: impl AsRef<Path>) -> io::Result<()> {
        let path = normalize(path.as_ref())?;
        TREE.with(|tree| {
            tree.borrow_mut()
                .files
                .remove(&path)
                .map(|_| ())
                .ok_or_else(missing)
        })
    }
    pub struct DirEntry {
        path: PathBuf,
    }
    impl DirEntry {
        pub fn path(&self) -> PathBuf {
            self.path.clone()
        }
        pub fn file_name(&self) -> OsString {
            self.path.file_name().unwrap_or_default().to_owned()
        }
    }
    pub fn read_dir(
        path: impl AsRef<Path>,
    ) -> io::Result<std::vec::IntoIter<io::Result<DirEntry>>> {
        let path = canonicalize(path)?;
        TREE.with(|tree| {
            let tree = tree.borrow();
            if !tree.dirs.contains(&path) {
                return Err(invalid("not a directory"));
            }
            let entries: BTreeSet<_> = tree
                .files
                .keys()
                .chain(tree.dirs.iter())
                .filter(|p| p.parent() == Some(path.as_path()))
                .cloned()
                .collect();
            Ok(entries
                .into_iter()
                .map(|path| Ok(DirEntry { path }))
                .collect::<Vec<_>>()
                .into_iter())
        })
    }
    pub struct File {
        bytes: Cursor<Vec<u8>>,
    }
    pub struct Metadata {
        len: u64,
    }
    impl Metadata {
        pub fn len(&self) -> u64 {
            self.len
        }
        pub fn is_empty(&self) -> bool {
            self.len == 0
        }
        pub fn is_file(&self) -> bool {
            true
        }
    }
    impl File {
        pub fn open(path: impl AsRef<Path>) -> io::Result<Self> {
            Ok(Self {
                bytes: Cursor::new(read(path)?),
            })
        }
        pub fn metadata(&self) -> io::Result<Metadata> {
            Ok(Metadata {
                len: self.bytes.get_ref().len() as u64,
            })
        }
    }
    impl Read for File {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            self.bytes.read(buf)
        }
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        #[test]
        fn untrusted_names_cannot_escape_virtual_storage() {
            reset();
            for path in [
                "relative.md",
                "/projects/../secret",
                "C:\\secret",
                "/a\\b",
                "/a\0b",
            ] {
                assert!(write(path, b"untrusted").is_err(), "accepted {path:?}");
            }
            assert!(list_files().is_empty());
        }

        #[test]
        fn retained_files_round_trip_without_sharing_mutable_bytes() {
            reset();
            write("/projects/p/sources/hash", b"original").unwrap();
            let mut copy = read("/projects/p/sources/hash").unwrap();
            copy.fill(0);
            assert_eq!(read("/projects/p/sources/hash").unwrap(), b"original");
            let saved: Vec<_> = list_files()
                .into_iter()
                .map(|p| {
                    let bytes = read(&p).unwrap();
                    (p, bytes)
                })
                .collect();
            reset();
            assert!(read("/projects/p/sources/hash").is_err());
            for (path, bytes) in saved {
                write(path, bytes).unwrap();
            }
            assert_eq!(
                canonicalize("/projects/p/sources").unwrap(),
                Path::new("/projects/p/sources")
            );
            assert_eq!(read("/projects/p/sources/hash").unwrap(), b"original");
        }

        #[test]
        fn files_and_directories_cannot_overwrite_each_other() {
            reset();
            write("/a/original", b"a").unwrap();
            assert!(write("/a", b"replacement").is_err());
            assert!(write("/a/original/child", b"replacement").is_err());
            let entries: Vec<_> = read_dir("/a").unwrap().map(|e| e.unwrap().path()).collect();
            assert_eq!(entries, vec![PathBuf::from("/a/original")]);
            assert_eq!(read("/a/original").unwrap(), b"a");
        }
    }
}
