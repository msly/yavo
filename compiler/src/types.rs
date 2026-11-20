use std::cell::RefCell;
use std::fmt;

thread_local! {
    /// User-defined structs and enums, indexed by id (`Ty::Struct(id)`, `Ty::Enum(id)` share one
    /// id space). The compiler checks one file per process, so a per-thread table keeps `Ty`
    /// small and `Copy`.
    static AGGS: RefCell<Vec<Agg>> = const { RefCell::new(Vec::new()) };
    /// Success and error type and C struct name of each `T ! E`, indexed by result id.
    static RESS: RefCell<Vec<(Ty, Ty, &'static str)>> = const { RefCell::new(Vec::new()) };
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
    /// Element type of each `[]T`, indexed by slice id.
    static SLICES: RefCell<Vec<Ty>> = const { RefCell::new(Vec::new()) };
    /// Element types and C struct name of each tuple type, indexed by tuple id. Elements are
    /// registered before the tuples that contain them.
    static TUPLES: RefCell<Vec<(Vec<Ty>, &'static str)>> = const { RefCell::new(Vec::new()) };
    /// Content type and C pointer type of each `Box[T]`, indexed by box id.
    static BOXES: RefCell<Vec<(Ty, &'static str)>> = const { RefCell::new(Vec::new()) };
    /// Name and C struct name of each interface, indexed by interface id.
    static IFACES: RefCell<Vec<(&'static str, &'static str)>> = const { RefCell::new(Vec::new()) };
}

/// Registers an interface name and returns its id (`Ty::Dyn(id)` is `Box[Name]`).
pub fn register_iface(name: &str) -> u32 {
    let c: &'static str = Box::leak(format!("yv_dyn_{name}").into_boxed_str());
    let name: &'static str = Box::leak(name.to_string().into_boxed_str());
    IFACES.with(|i| {
        let mut i = i.borrow_mut();
        i.push((name, c));
        (i.len() - 1) as u32
    })
}

pub fn iface_name(id: u32) -> &'static str {
    IFACES.with(|i| i.borrow()[id as usize].0)
}

/// Interns `Box[inner]` and returns its id. In C a box is a pointer to one heap block; a
/// struct-like content is named by its struct tag, so the pointer type can be written before
/// the content's definition (`struct Node { left: ?Box[Node] }`).
pub fn box_of(inner: Ty) -> u32 {
    let c_name = match inner {
        Ty::Struct(_) | Ty::Enum(_) | Ty::Opt(_) | Ty::Res(_) | Ty::Tuple(_) => format!("struct {} *", inner.c_name()),
        _ => format!("{} *", inner.c_name()),
    };
    BOXES.with(|b| {
        let mut b = b.borrow_mut();
        if let Some(i) = b.iter().position(|(t, _)| *t == inner) {
            return i as u32;
        }
        let c: &'static str = Box::leak(c_name.into_boxed_str());
        b.push((inner, c));
        (b.len() - 1) as u32
    })
}

pub fn box_inner(id: u32) -> Ty {
    BOXES.with(|b| b.borrow()[id as usize].0)
}

/// Every `Box` type created so far.
pub fn all_boxes() -> Vec<(u32, Ty)> {
    BOXES.with(|b| b.borrow().iter().enumerate().map(|(i, (t, _))| (i as u32, *t)).collect())
}

/// Interns the tuple type `(elems...)` and returns its id.
pub fn tuple_of(elems: Vec<Ty>) -> u32 {
    if let Some(i) = TUPLES.with(|t| t.borrow().iter().position(|(e, _)| *e == elems)) {
        return i as u32;
    }
    let key: Vec<String> = elems.iter().map(|t| t.key()).collect();
    let c: &'static str = Box::leak(format!("yv_t_{}", key.join("__")).into_boxed_str());
    TUPLES.with(|t| {
        let mut t = t.borrow_mut();
        t.push((elems, c));
        (t.len() - 1) as u32
    })
}

pub fn tuple_elems(id: u32) -> Vec<Ty> {
    TUPLES.with(|t| t.borrow()[id as usize].0.clone())
}

pub fn all_tuples() -> Vec<u32> {
    TUPLES.with(|t| (0..t.borrow().len() as u32).collect())
}

/// Interns `[]elem` and returns its id.
pub fn slice_of(elem: Ty) -> u32 {
    SLICES.with(|v| {
        let mut v = v.borrow_mut();
        if let Some(i) = v.iter().position(|t| *t == elem) {
            return i as u32;
        }
        v.push(elem);
        (v.len() - 1) as u32
    })
}

pub fn slice_elem(id: u32) -> Ty {
    SLICES.with(|v| v.borrow()[id as usize])
}

/// A struct or an enum. The flags are filled in by `finish_agg` once the member types are
/// known; until then the aggregate counts as a plain copyable value.
struct Agg {
    name: &'static str,
    c_name: &'static str,
    /// Struct fields, or the payload fields of every enum case (one list per case).
    members: Vec<Vec<Ty>>,
    copy: bool,
    drop: bool,
    /// `struct T: Drop`: the user's `drop` runs first. Such a value has a hidden `yv_live` flag
    /// (all-zero, i.e. moved out, means "nothing to drop").
    user_drop: bool,
}

fn register_agg(name: &str, prefix: &str) -> u32 {
    let c_name: &'static str = Box::leak(format!("{prefix}{}", c_safe(name)).into_boxed_str());
    let name: &'static str = Box::leak(name.to_string().into_boxed_str());
    AGGS.with(|s| {
        let mut s = s.borrow_mut();
        s.push(Agg { name, c_name, members: Vec::new(), copy: true, drop: false, user_drop: false });
        (s.len() - 1) as u32
    })
}

/// Registers a struct name and returns its id.
pub fn register_struct(name: &str) -> u32 {
    register_agg(name, "yv_s_")
}

/// A type name usable in C identifiers: an instance name such as `Pair[int, String]` becomes
/// `Pair_int_String_`.
fn c_safe(name: &str) -> String {
    name.chars().filter(|c| *c != ' ').map(|c| if c.is_ascii_alphanumeric() || c == '_' { c } else { '_' }).collect()
}

/// Registers an enum name and returns its id. A dot in a library type name (`fs.Error`) becomes
/// `_` in C.
pub fn register_enum(name: &str) -> u32 {
    let id = register_agg(name, "yv_e_");
    if name.contains('.') {
        let c: &'static str = Box::leak(format!("yv_e_{}", name.replace('.', "_")).into_boxed_str());
        AGGS.with(|s| s.borrow_mut()[id as usize].c_name = c);
    }
    id
}

/// Marks an aggregate as having a user `drop` (call before `finish_agg`).
pub fn set_user_drop(id: u32) {
    AGGS.with(|s| s.borrow_mut()[id as usize].user_drop = true);
}

/// Whether an aggregate type has a user `drop`.
pub fn has_user_drop(t: Ty) -> bool {
    match t {
        Ty::Struct(id) | Ty::Enum(id) => AGGS.with(|s| s.borrow()[id as usize].user_drop),
        _ => false,
    }
}

/// Records the member types of an aggregate (struct fields as one list, or one list per enum
/// case) and derives whether it is copyable and whether it must be dropped. Members are
/// finished first (the checker walks aggregates in dependency order).
pub fn finish_agg(id: u32, members: Vec<Vec<Ty>>) {
    let user_drop = AGGS.with(|s| s.borrow()[id as usize].user_drop);
    let copy = !user_drop && members.iter().flatten().all(|t| t.is_copy());
    let drop = user_drop || members.iter().flatten().any(|t| t.needs_drop());
    AGGS.with(|s| {
        let mut s = s.borrow_mut();
        let a = &mut s[id as usize];
        a.members = members;
        a.copy = copy;
        a.drop = drop;
    })
}

fn agg_names(id: u32) -> (&'static str, &'static str) {
    AGGS.with(|s| {
        let a = &s.borrow()[id as usize];
        (a.name, a.c_name)
    })
}

fn agg_flags(id: u32) -> (bool, bool) {
    AGGS.with(|s| {
        let a = &s.borrow()[id as usize];
        (a.copy, a.drop)
    })
}

/// Interns `ok ! err` and returns its id.
pub fn res_of(ok: Ty, err: Ty) -> u32 {
    let key = format!("{}__{}", ok.key(), err.key());
    RESS.with(|r| {
        let mut r = r.borrow_mut();
        if let Some(i) = r.iter().position(|(o, e, _)| *o == ok && *e == err) {
            return i as u32;
        }
        let c: &'static str = Box::leak(format!("yv_res_{key}").into_boxed_str());
        r.push((ok, err, c));
        (r.len() - 1) as u32
    })
}

pub fn res_parts(id: u32) -> (Ty, Ty) {
    RESS.with(|r| {
        let (o, e, _) = r.borrow()[id as usize];
        (o, e)
    })
}

/// Every `T ! E` type created so far, in dependency order (parts first).
pub fn all_ress() -> Vec<(u32, Ty, Ty)> {
    RESS.with(|r| r.borrow().iter().enumerate().map(|(i, (o, e, _))| (i as u32, *o, *e)).collect())
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
        // A pointer type (`Box[T]`) takes the `const` after it, on the pointer itself.
        let c = if c_name.ends_with('*') { format!("{c_name} const *") } else { format!("const {c_name} *") };
        let c: &'static str = Box::leak(c.into_boxed_str());
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Ty {
    /// `f32` or `f64` (IEEE 754). Copyable; not `Eq`/`Ord` (K1).
    Float(FloatTy),
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
    /// `Set[K]`, by the map id of `Map[K, bool]`: an insertion-ordered set stored as that map
    /// (same C type, helpers and key), with its own methods.
    Set(u32),
    /// `[]T`: a read-only view of consecutive elements, by slice id (see `slice_of`). Copyable;
    /// it carries a borrow. In C a `yv_vec` whose storage belongs to someone else.
    Slice(u32),
    Unit,
    /// A user-defined struct, by id (see `register_struct`).
    Struct(u32),
    /// A user-defined or library enum, by id (see `register_enum`). In C a `{ uint32_t tag;
    /// union { ... } u; }` struct; tag 0 with a zero payload is the first case.
    Enum(u32),
    /// `T ! E`: a success value or an error, by result id (see `res_of`). In C a `{ bool ok;
    /// T val; E err; }` struct (no `val` when `T` is `()`); all-zero is an error holding a
    /// zero value, which is safe to drop.
    Res(u32),
    /// `(A, B, ...)`, by tuple id (see `tuple_of`). In C a struct with members `f0`, `f1`, ...
    Tuple(u32),
    /// `Box[T]`: one owned heap block holding a `T`, by box id (see `box_of`). In C a `T *`;
    /// NULL (all-zero) is a moved-out box, which drops nothing.
    Box(u32),
    /// `Box[I]` for an interface `I`, by interface id (see `register_iface`): a boxed value of
    /// some type that implements `I`. In C a `{ void *data; const void *vt; }` pair; a NULL
    /// `data` is a moved-out value.
    Dyn(u32),
    /// Produced after an error has been reported; suppresses cascading diagnostics.
    Error,
}

impl Ty {
    /// Copyable values are duplicated by assignment; the others (`String`, `Vec`, aggregates
    /// holding them) are moved.
    pub fn is_copy(self) -> bool {
        match self {
            Ty::String | Ty::Vec(_) | Ty::Map(_) | Ty::Set(_) | Ty::RefMut(_) | Ty::Box(_) | Ty::Dyn(_) => false,
            Ty::Opt(id) => opt_inner(id).is_copy(),
            Ty::Tuple(id) => tuple_elems(id).iter().all(|t| t.is_copy()),
            Ty::Struct(id) | Ty::Enum(id) => agg_flags(id).0,
            Ty::Res(id) => {
                let (o, e) = res_parts(id);
                o.is_copy() && e.is_copy()
            }
            _ => true,
        }
    }

    /// Whether a value of this type owns heap memory that must be released when it is dropped.
    /// `&inout T` is not copyable but owns nothing.
    pub fn needs_drop(self) -> bool {
        match self {
            Ty::String | Ty::Vec(_) | Ty::Map(_) | Ty::Set(_) | Ty::Box(_) | Ty::Dyn(_) => true,
            Ty::Opt(id) => opt_inner(id).needs_drop(),
            Ty::Tuple(id) => tuple_elems(id).iter().any(|t| t.needs_drop()),
            Ty::Struct(id) | Ty::Enum(id) => agg_flags(id).1,
            Ty::Res(id) => {
                let (o, e) = res_parts(id);
                o.needs_drop() || e.needs_drop()
            }
            _ => false,
        }
    }

    /// Whether this type contains an `&inout T` (such values keep exclusive access).
    pub fn contains_ref_mut(self) -> bool {
        match self {
            Ty::RefMut(_) => true,
            Ty::Opt(id) => opt_inner(id).contains_ref_mut(),
            Ty::Box(id) => box_inner(id).contains_ref_mut(),
            Ty::Tuple(id) => tuple_elems(id).iter().any(|t| t.contains_ref_mut()),
            Ty::Vec(id) => vec_elem(id).contains_ref_mut(),
            Ty::Slice(id) => slice_elem(id).contains_ref_mut(),
            Ty::Res(id) => {
                let (o, e) = res_parts(id);
                o.contains_ref_mut() || e.contains_ref_mut()
            }
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
            Ty::Str | Ty::Ref(_) | Ty::RefMut(_) | Ty::Slice(_) => true,
            Ty::Vec(id) => vec_elem(id).carries_borrow(),
            Ty::Set(id) => map_parts(id).0.carries_borrow(),
            Ty::Box(id) => box_inner(id).carries_borrow(),
            Ty::Opt(id) => opt_inner(id).carries_borrow(),
            Ty::Tuple(id) => tuple_elems(id).iter().any(|t| t.carries_borrow()),
            Ty::Res(id) => {
                let (o, e) = res_parts(id);
                o.carries_borrow() || e.carries_borrow()
            }
            Ty::Map(id) => {
                let (k, v) = map_parts(id);
                k.carries_borrow() || v.carries_borrow()
            }
            _ => false,
        }
    }

    /// `take(&x)` leaves this value behind (stdlib contract §1). In C it is all-zero bytes.
    pub fn has_default(self) -> bool {
        matches!(self, Ty::Int(_) | Ty::Float(_) | Ty::Bool | Ty::String | Ty::Vec(_) | Ty::Opt(_) | Ty::Map(_) | Ty::Set(_))
    }


    /// A name usable inside C identifiers, e.g. `Vec_Vec_String` for `Vec[Vec[String]]`.
    pub fn key(self) -> String {
        match self {
            Ty::Int(t) => t.suffix().to_string(),
            Ty::Float(f) => f.name().to_string(),
            Ty::Bool => "bool".into(),
            Ty::Str => "str".into(),
            Ty::String => "String".into(),
            Ty::Vec(id) => format!("Vec_{}", vec_elem(id).key()),
            Ty::Slice(id) => format!("Slice_{}", slice_elem(id).key()),
            Ty::Ref(id) => format!("Ref_{}", ref_inner(id).key()),
            Ty::Opt(id) => format!("Opt_{}", opt_inner(id).key()),
            Ty::RefMut(id) => format!("RefMut_{}", ref_mut_inner(id).key()),
            Ty::Box(id) => format!("Box_{}", box_inner(id).key()),
            Ty::Dyn(id) => format!("Dyn_{}", iface_name(id)),
            // A set shares its map's key, so it shares the map's C helpers.
            Ty::Map(id) | Ty::Set(id) => {
                let (k, v) = map_parts(id);
                format!("Map_{}_{}", k.key(), v.key())
            }
            Ty::Struct(id) => format!("S_{}", c_safe(agg_names(id).0)),
            Ty::Enum(id) => format!("E_{}", agg_names(id).0.replace('.', "_")),
            Ty::Res(id) => {
                let (o, e) = res_parts(id);
                format!("Res_{}__{}", o.key(), e.key())
            }
            Ty::Tuple(id) => {
                let parts: Vec<String> = tuple_elems(id).iter().map(|t| t.key()).collect();
                format!("Tup_{}_End", parts.join("_"))
            }
            Ty::Unit | Ty::Error => "void".into(),
        }
    }

    pub fn c_name(self) -> &'static str {
        match self {
            Ty::Int(t) => t.c_name(),
            Ty::Float(f) => f.c_name(),
            Ty::Bool => "bool",
            Ty::Str => "yv_str",
            Ty::String => "yv_string",
            Ty::Vec(_) | Ty::Slice(_) => "yv_vec",
            Ty::Ref(id) => REFS.with(|r| r.borrow()[id as usize].1),
            Ty::Opt(id) => OPTS.with(|o| o.borrow()[id as usize].1),
            Ty::RefMut(id) => REFMUTS.with(|r| r.borrow()[id as usize].1),
            Ty::Map(_) | Ty::Set(_) => "yv_map",
            Ty::Unit => "void",
            Ty::Struct(id) | Ty::Enum(id) => agg_names(id).1,
            Ty::Res(id) => RESS.with(|r| r.borrow()[id as usize].2),
            Ty::Tuple(id) => TUPLES.with(|t| t.borrow()[id as usize].1),
            Ty::Box(id) => BOXES.with(|b| b.borrow()[id as usize].1),
            Ty::Dyn(id) => IFACES.with(|i| i.borrow()[id as usize].1),
            Ty::Error => "void",
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int(t) => write!(f, "{t}"),
            Ty::Float(t) => f.write_str(t.name()),
            Ty::Bool => f.write_str("bool"),
            Ty::Str => f.write_str("str"),
            Ty::String => f.write_str("String"),
            Ty::Vec(id) => write!(f, "Vec[{}]", vec_elem(*id)),
            Ty::Slice(id) => write!(f, "[]{}", slice_elem(*id)),
            Ty::Ref(id) => write!(f, "&{}", ref_inner(*id)),
            Ty::Opt(id) => write!(f, "?{}", opt_inner(*id)),
            Ty::RefMut(id) => write!(f, "&inout {}", ref_mut_inner(*id)),
            Ty::Box(id) => write!(f, "Box[{}]", box_inner(*id)),
            Ty::Dyn(id) => write!(f, "Box[{}]", iface_name(*id)),
            Ty::Map(id) => {
                let (k, v) = map_parts(*id);
                write!(f, "Map[{k}, {v}]")
            }
            Ty::Set(id) => write!(f, "Set[{}]", map_parts(*id).0),
            Ty::Unit => f.write_str("()"),
            Ty::Struct(id) | Ty::Enum(id) => f.write_str(agg_names(*id).0),
            Ty::Res(id) => {
                let (o, e) = res_parts(*id);
                write!(f, "{o} ! {e}")
            }
            Ty::Tuple(id) => {
                let parts: Vec<String> = tuple_elems(*id).iter().map(|t| t.to_string()).collect();
                write!(f, "({})", parts.join(", "))
            }
            Ty::Error => f.write_str("{error}"),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FloatTy {
    F32,
    F64,
}

impl FloatTy {
    pub fn from_name(name: &str) -> Option<FloatTy> {
        match name {
            "f32" => Some(FloatTy::F32),
            "f64" => Some(FloatTy::F64),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            FloatTy::F32 => "f32",
            FloatTy::F64 => "f64",
        }
    }

    pub fn c_name(self) -> &'static str {
        match self {
            FloatTy::F32 => "float",
            FloatTy::F64 => "double",
        }
    }
}

impl fmt::Display for FloatTy {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.name())
    }
}
