//! `ra_hir_def` contains initial "phases" of the compiler. Roughly, everything
//! before types.
//!
//! Note that we are in the process of moving parts of `ra_hir` into
//! `ra_hir_def`, so this crates doesn't contain a lot at the moment.

pub mod db;
pub mod ast_id_map;

use std::hash::{Hash, Hasher};

use ra_db::{salsa, CrateId, FileId};
use ra_syntax::ast::{self, AstNode};

use crate::{ast_id_map::FileAstId, db::AstDatabase};

macro_rules! impl_intern_key {
    ($name:ident) => {
        impl salsa::InternKey for $name {
            fn from_intern_id(v: salsa::InternId) -> Self {
                $name(v)
            }
            fn as_intern_id(&self) -> salsa::InternId {
                self.0
            }
        }
    };
}

/// Input to the analyzer is a set of files, where each file is identified by
/// `FileId` and contains source code. However, another source of source code in
/// Rust are macros: each macro can be thought of as producing a "temporary
/// file". To assign an id to such a file, we use the id of the macro call that
/// produced the file. So, a `HirFileId` is either a `FileId` (source code
/// written by user), or a `MacroCallId` (source code produced by macro).
///
/// What is a `MacroCallId`? Simplifying, it's a `HirFileId` of a file
/// containing the call plus the offset of the macro call in the file. Note that
/// this is a recursive definition! However, the size_of of `HirFileId` is
/// finite (because everything bottoms out at the real `FileId`) and small
/// (`MacroCallId` uses the location interner).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct HirFileId(HirFileIdRepr);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
enum HirFileIdRepr {
    FileId(FileId),
    MacroFile(MacroFile),
}

impl From<FileId> for HirFileId {
    fn from(id: FileId) -> Self {
        HirFileId(HirFileIdRepr::FileId(id))
    }
}

impl From<MacroFile> for HirFileId {
    fn from(id: MacroFile) -> Self {
        HirFileId(HirFileIdRepr::MacroFile(id))
    }
}

impl HirFileId {
    /// For macro-expansion files, returns the file original source file the
    /// expansion originated from.
    pub fn original_file(self, db: &impl AstDatabase) -> FileId {
        match self.0 {
            HirFileIdRepr::FileId(file_id) => file_id,
            HirFileIdRepr::MacroFile(macro_file) => {
                let loc = db.lookup_intern_macro(macro_file.macro_call_id);
                loc.ast_id.file_id().original_file(db)
            }
        }
    }

    /// Get the crate which the macro lives in, if it is a macro file.
    pub fn macro_crate(self, db: &impl AstDatabase) -> Option<CrateId> {
        match self.0 {
            HirFileIdRepr::FileId(_) => None,
            HirFileIdRepr::MacroFile(macro_file) => {
                let loc = db.lookup_intern_macro(macro_file.macro_call_id);
                Some(loc.def.krate)
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacroFile {
    macro_call_id: MacroCallId,
    macro_file_kind: MacroFileKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MacroFileKind {
    Items,
    Expr,
}

/// `MacroCallId` identifies a particular macro invocation, like
/// `println!("Hello, {}", world)`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacroCallId(salsa::InternId);
impl_intern_key!(MacroCallId);

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct MacroDefId {
    pub krate: CrateId,
    pub ast_id: AstId<ast::MacroCall>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct MacroCallLoc {
    pub def: MacroDefId,
    pub ast_id: AstId<ast::MacroCall>,
}

impl MacroCallId {
    pub fn loc(self, db: &impl AstDatabase) -> MacroCallLoc {
        db.lookup_intern_macro(self)
    }

    pub fn as_file(self, kind: MacroFileKind) -> HirFileId {
        let macro_file = MacroFile { macro_call_id: self, macro_file_kind: kind };
        macro_file.into()
    }
}

impl MacroCallLoc {
    pub fn id(self, db: &impl AstDatabase) -> MacroCallId {
        db.intern_macro(self)
    }
}

/// `AstId` points to an AST node in any file.
///
/// It is stable across reparses, and can be used as salsa key/value.
// FIXME: isn't this just a `Source<FileAstId<N>>` ?
#[derive(Debug)]
pub struct AstId<N: AstNode> {
    file_id: HirFileId,
    file_ast_id: FileAstId<N>,
}

impl<N: AstNode> Clone for AstId<N> {
    fn clone(&self) -> AstId<N> {
        *self
    }
}
impl<N: AstNode> Copy for AstId<N> {}

impl<N: AstNode> PartialEq for AstId<N> {
    fn eq(&self, other: &Self) -> bool {
        (self.file_id, self.file_ast_id) == (other.file_id, other.file_ast_id)
    }
}
impl<N: AstNode> Eq for AstId<N> {}
impl<N: AstNode> Hash for AstId<N> {
    fn hash<H: Hasher>(&self, hasher: &mut H) {
        (self.file_id, self.file_ast_id).hash(hasher);
    }
}

impl<N: AstNode> AstId<N> {
    pub fn new(file_id: HirFileId, file_ast_id: FileAstId<N>) -> AstId<N> {
        AstId { file_id, file_ast_id }
    }

    pub fn file_id(&self) -> HirFileId {
        self.file_id
    }

    pub fn to_node(&self, db: &impl AstDatabase) -> N {
        let syntax_node = db.ast_id_to_node(self.file_id, self.file_ast_id.into());
        N::cast(syntax_node).unwrap()
    }
}
