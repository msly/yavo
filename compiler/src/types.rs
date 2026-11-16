use std::cell::RefCell;
use std::fmt;

thread_local! {
    /// Struct names and their C names, indexed by struct id. The compiler checks one file per
    /// process, so a per-thread table keeps `Ty` small and `Copy`.
    static STRUCTS: RefCell<Vec<(&'static str, &'static str)>> = const { RefCell::new(Vec::new()) };
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
    Unit,
    /// A user-defined struct, by id (see `register_struct`).
    Struct(u32),
    /// Produced after an error has been reported; suppresses cascading diagnostics.
    Error,
}

impl Ty {
    pub fn c_name(self) -> &'static str {
        match self {
            Ty::Int(t) => t.c_name(),
            Ty::Bool => "bool",
            Ty::Str => "yv_str",
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
            Ty::Unit => f.write_str("()"),
            Ty::Struct(id) => f.write_str(struct_names(*id).0),
            Ty::Error => f.write_str("{error}"),
        }
    }
}
