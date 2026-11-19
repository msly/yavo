use std::cell::RefCell;
use std::fmt;

thread_local! {
    /// Struct names and their C names, indexed by struct id. The compiler checks one file per
    /// process, so a per-thread table keeps `Ty` small and `Copy`.
    static STRUCTS: RefCell<Vec<(&'static str, &'static str)>> = const { RefCell::new(Vec::new()) };
    /// Element type of each `Vec[T]`, indexed by vec id. An element is always registered
    /// before the vectors that contain it, so ids are in dependency order.
    static VECS: RefCell<Vec<Ty>> = const { RefCell::new(Vec::new()) };
    /// Referent type and C pointer type of each `&T`, indexed by ref id.
    static REFS: RefCell<Vec<(Ty, &'static str)>> = const { RefCell::new(Vec::new()) };
    /// Payload type and C struct name of each `?T`, indexed by opt id. A payload is always
    /// registered before the optionals that contain it.
    static OPTS: RefCell<Vec<(Ty, &'static str)>> = const { RefCell::new(Vec::new()) };
    /// Referent type and C pointer type of each `&inout T`, indexed by id.
    static REFMUTS: RefCell<Vec<(Ty, &'static str)>> = const { RefCell::new(Vec::new()) };
    /// Key and value type of each `Map[K, V]`, indexed by map id; parts are registered first.
    static MAPS: RefCell<Vec<(Ty, Ty)>> = const { RefCell::new(Vec::new()) };
}

/// Registers a struct name and returns its id.
pub fn register_struct(name: &str) -> u32 {
    let name: &'static str = Box::leak(name.to_string().into_boxed_str());
    let c_name: &'static str = Box::leak(format!("yv_s_{name}").into_boxed_str());
    STRUCTS.with(|s| {
        let mut s = s.borrow_mut();
        s.push((name, c_name));
        (s.len() - 1) as u32
    })
}

fn struct_names(id: u32) -> (&'static str, &'static str) {
    STRUCTS.with(|s| s.borrow()[id as usize])
}

/// Interns `Vec[elem]` and returns its id.
pub fn vec_of(elem: Ty) -> u32 {
    VECS.with(|v| {
        let mut v = v.borrow_mut();
        if let Some(i) = v.iter().position(|t| *t == elem) {
            return i as u32;
        }
        v.push(elem);
        (v.len() - 1) as u32
    })
}

pub fn vec_elem(id: u32) -> Ty {
    VECS.with(|v| v.borrow()[id as usize])
}

/// Interns `&inner` and returns its id.
pub fn ref_of(inner: Ty) -> u32 {
    // Computed before borrowing the table: `c_name` of a nested `&T` reads the same table.
    let c_name = inner.c_name();
    REFS.with(|r| {
        let mut r = r.borrow_mut();
        if let Some(i) = r.iter().position(|(t, _)| *t == inner) {
            return i as u32;
        }
        let c: &'static str = Box::leak(format!("const {c_name} *").into_boxed_str());
        r.push((inner, c));
        (r.len() - 1) as u32
    })
}

pub fn ref_inner(id: u32) -> Ty {
    REFS.with(|r| r.borrow()[id as usize].0)
}

/// Interns `?inner` and returns its id.
pub fn opt_of(inner: Ty) -> u32 {
    // Computed before borrowing the table: the key of a nested `??T` reads the same table.
    let key = inner.key();
    OPTS.with(|o| {
        let mut o = o.borrow_mut();
        if let Some(i) = o.iter().position(|(t, _)| *t == inner) {
            return i as u32;
        }
        let c: &'static str = Box::leak(format!("yv_opt_{key}").into_boxed_str());
        o.push((inner, c));
        (o.len() - 1) as u32
    })
}

pub fn opt_inner(id: u32) -> Ty {
    OPTS.with(|o| o.borrow()[id as usize].0)
}

/// Every `?T` type created so far, in dependency order (payloads first).
pub fn all_opts() -> Vec<(u32, Ty)> {
    OPTS.with(|o| o.borrow().iter().enumerate().map(|(i, (t, _))| (i as u32, *t)).collect())
}

/// Interns `&inout inner` and returns its id.
pub fn ref_mut_of(inner: Ty) -> u32 {
    let c_name = inner.c_name();
    REFMUTS.with(|r| {
        let mut r = r.borrow_mut();
        if let Some(i) = r.iter().position(|(t, _)| *t == inner) {
            return i as u32;
        }
        let c: &'static str = Box::leak(format!("{c_name} *").into_boxed_str());
        r.push((inner, c));
        (r.len() - 1) as u32
    })
}

pub fn ref_mut_inner(id: u32) -> Ty {
    REFMUTS.with(|r| r.borrow()[id as usize].0)
}

/// Interns `Map[key, value]` and returns its id.
pub fn map_of(key: Ty, value: Ty) -> u32 {
    MAPS.with(|m| {
        let mut m = m.borrow_mut();
        if let Some(i) = m.iter().position(|kv| *kv == (key, value)) {
            return i as u32;
        }
        m.push((key, value));
        (m.len() - 1) as u32
    })
}

pub fn map_parts(id: u32) -> (Ty, Ty) {
    MAPS.with(|m| m.borrow()[id as usize])
}

/// Every `Map` type created so far, in dependency order.
pub fn all_maps() -> Vec<(u32, Ty, Ty)> {
    MAPS.with(|m| m.borrow().iter().enumerate().map(|(i, (k, v))| (i as u32, *k, *v)).collect())
}

/// Every `Vec` type created so far, in dependency order.
pub fn all_vecs() -> Vec<(u32, Ty)> {
    VECS.with(|v| v.borrow().iter().enumerate().map(|(i, t)| (i as u32, *t)).collect())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum IntTy {
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
}

impl IntTy {
    pub fn from_name(name: &str) -> Option<IntTy> {
        Some(match name {
            "i8" => IntTy::I8,
            "i16" => IntTy::I16,
            "i32" => IntTy::I32,
            "i64" | "int" => IntTy::I64,
            "u8" | "byte" => IntTy::U8,
            "u16" => IntTy::U16,
            "u32" => IntTy::U32,
            "u64" => IntTy::U64,
            _ => return None,
        })
    }

    pub fn bits(self) -> u32 {
        match self {
            IntTy::I8 | IntTy::U8 => 8,
            IntTy::I16 | IntTy::U16 => 16,
            IntTy::I32 | IntTy::U32 => 32,
            IntTy::I64 | IntTy::U64 => 64,
        }
    }

    pub fn signed(self) -> bool {
        matches!(self, IntTy::I8 | IntTy::I16 | IntTy::I32 | IntTy::I64)
    }

    pub fn min(self) -> i128 {
        if self.signed() {
            -(1i128 << (self.bits() - 1))
        } else {
            0
        }
    }

    pub fn max(self) -> i128 {
        if self.signed() {
            (1i128 << (self.bits() - 1)) - 1
        } else {
            (1i128 << self.bits()) - 1
        }
    }

    /// Suffix used to select runtime helpers, e.g. `yv_add_i32`.
    pub fn suffix(self) -> &'static str {
        match self {
            IntTy::I8 => "i8",
            IntTy::I16 => "i16",
            IntTy::I32 => "i32",
            IntTy::I64 => "i64",
            IntTy::U8 => "u8",
            IntTy::U16 => "u16",
            IntTy::U32 => "u32",
            IntTy::U64 => "u64",
        }
    }

    pub fn c_name(self) -> &'static str {
        match self {
            IntTy::I8 => "int8_t",
            IntTy::I16 => "int16_t",
            IntTy::I32 => "int32_t",
            IntTy::I64 => "int64_t",
            IntTy::U8 => "uint8_t",
            IntTy::U16 => "uint16_t",
            IntTy::U32 => "uint32_t",
            IntTy::U64 => "uint64_t",
        }
    }
}

impl fmt::Display for IntTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // `int` is the spelling the cheatsheet teaches; it is the same type as i64.
        match self {
            IntTy::I64 => f.write_str("int"),
            other => f.write_str(other.suffix()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Ty {
    Int(IntTy),
    Bool,
    Str,
    /// Owned, growable UTF-8 text.
    String,
    /// `Vec[T]`, by vec id (see `vec_of`).
    Vec(u32),
    /// `&T`: a read-only reference, by ref id (see `ref_of`). Copyable; it carries a borrow.
    Ref(u32),
    /// `?T`: a value that may be absent, by opt id (see `opt_of`). In C a `{ bool has; T val; }`
    /// struct; all-zero is `none`, so a moved-out optional is `none`.
    Opt(u32),
    /// `&inout T`: an exclusive, writable reference, by id (see `ref_mut_of`). Not copyable: it
    /// moves. In C a `T *`. Produced by `getMut` and `for x in &v`.
    RefMut(u32),
    /// `Map[K, V]`, by map id (see `map_of`): an insertion-ordered hash map (`yv_map` in C).
    Map(u32),
    Unit,
    /// A user-defined struct, by id (see `register_struct`).
    Struct(u32),
    /// Produced after an error has been reported; suppresses cascading diagnostics.
    Error,
}

impl Ty {
    /// Copyable values are duplicated by assignment; the others (`String`, `Vec`) are moved.
    pub fn is_copy(self) -> bool {
        match self {
            Ty::String | Ty::Vec(_) | Ty::Map(_) | Ty::RefMut(_) => false,
            Ty::Opt(id) => opt_inner(id).is_copy(),
            _ => true,
        }
    }

    /// Whether a value of this type owns heap memory that must be released when it is dropped.
    /// `&inout T` is not copyable but owns nothing.
    pub fn needs_drop(self) -> bool {
        match self {
            Ty::String | Ty::Vec(_) | Ty::Map(_) => true,
            Ty::Opt(id) => opt_inner(id).needs_drop(),
            _ => false,
        }
    }

    /// Whether this type contains an `&inout T` (such values keep exclusive access).
    pub fn contains_ref_mut(self) -> bool {
        match self {
            Ty::RefMut(_) => true,
            Ty::Opt(id) => opt_inner(id).contains_ref_mut(),
            Ty::Vec(id) => vec_elem(id).contains_ref_mut(),
            Ty::Map(id) => {
                let (k, v) = map_parts(id);
                k.contains_ref_mut() || v.contains_ref_mut()
            }
            _ => false,
        }
    }

    /// Map keys v1 can hash (stdlib contract §3; `Id` and tuples are not implemented yet).
    pub fn is_hashable(self) -> bool {
        matches!(self, Ty::Int(_) | Ty::Bool | Ty::Str | Ty::String)
    }

    /// Whether a value of this type points into data it does not own (`str`, `Vec[str]`, ...).
    pub fn carries_borrow(self) -> bool {
        match self {
            Ty::Str | Ty::Ref(_) | Ty::RefMut(_) => true,
            Ty::Vec(id) => vec_elem(id).carries_borrow(),
            Ty::Opt(id) => opt_inner(id).carries_borrow(),
            Ty::Map(id) => {
                let (k, v) = map_parts(id);
                k.carries_borrow() || v.carries_borrow()
            }
            _ => false,
        }
    }

    /// `take(&x)` leaves this value behind (stdlib contract §1). In C it is all-zero bytes.
    pub fn has_default(self) -> bool {
        matches!(self, Ty::Int(_) | Ty::Bool | Ty::String | Ty::Vec(_) | Ty::Opt(_) | Ty::Map(_))
    }

    /// A name usable inside C identifiers, e.g. `Vec_Vec_String` for `Vec[Vec[String]]`.
    pub fn key(self) -> String {
        match self {
            Ty::Int(t) => t.suffix().to_string(),
            Ty::Bool => "bool".into(),
            Ty::Str => "str".into(),
            Ty::String => "String".into(),
            Ty::Vec(id) => format!("Vec_{}", vec_elem(id).key()),
            Ty::Ref(id) => format!("Ref_{}", ref_inner(id).key()),
            Ty::Opt(id) => format!("Opt_{}", opt_inner(id).key()),
            Ty::RefMut(id) => format!("RefMut_{}", ref_mut_inner(id).key()),
            Ty::Map(id) => {
                let (k, v) = map_parts(id);
                format!("Map_{}_{}", k.key(), v.key())
            }
            Ty::Struct(id) => format!("S_{}", struct_names(id).0),
            Ty::Unit | Ty::Error => "void".into(),
        }
    }

    pub fn c_name(self) -> &'static str {
        match self {
            Ty::Int(t) => t.c_name(),
            Ty::Bool => "bool",
            Ty::Str => "yv_str",
            Ty::String => "yv_string",
            Ty::Vec(_) => "yv_vec",
            Ty::Ref(id) => REFS.with(|r| r.borrow()[id as usize].1),
            Ty::Opt(id) => OPTS.with(|o| o.borrow()[id as usize].1),
            Ty::RefMut(id) => REFMUTS.with(|r| r.borrow()[id as usize].1),
            Ty::Map(_) => "yv_map",
            Ty::Unit => "void",
            Ty::Struct(id) => struct_names(id).1,
            Ty::Error => "void",
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int(t) => write!(f, "{t}"),
            Ty::Bool => f.write_str("bool"),
            Ty::Str => f.write_str("str"),
            Ty::String => f.write_str("String"),
            Ty::Vec(id) => write!(f, "Vec[{}]", vec_elem(*id)),
            Ty::Ref(id) => write!(f, "&{}", ref_inner(*id)),
            Ty::Opt(id) => write!(f, "?{}", opt_inner(*id)),
            Ty::RefMut(id) => write!(f, "&inout {}", ref_mut_inner(*id)),
            Ty::Map(id) => {
                let (k, v) = map_parts(*id);
                write!(f, "Map[{k}, {v}]")
            }
            Ty::Unit => f.write_str("()"),
            Ty::Struct(id) => f.write_str(struct_names(*id).0),
            Ty::Error => f.write_str("{error}"),
        }
    }
}
