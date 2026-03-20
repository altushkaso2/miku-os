const MAX_SYMS: usize = 1024;

struct Sym {
    name:  *const u8,
    value: u64,
    weak:  bool,
}

struct SymTab {
    syms:  [Sym; MAX_SYMS],
    count: usize,
}

unsafe impl Send for SymTab {}
unsafe impl Sync for SymTab {}

impl SymTab {
    const fn new() -> Self {
        const EMPTY: Sym = Sym { name: core::ptr::null(), value: 0, weak: false };
        Self { syms: [EMPTY; MAX_SYMS], count: 0 }
    }

    fn export(&mut self, name: *const u8, value: u64, weak: bool) {
        if name.is_null() { return; }
        for i in 0..self.count {
            if crate::util::streq(self.syms[i].name, name) {
                if !self.syms[i].weak { return; }
                self.syms[i].value = value;
                self.syms[i].weak  = weak;
                return;
            }
        }
        if self.count >= MAX_SYMS { return; }
        self.syms[self.count] = Sym { name, value, weak };
        self.count += 1;
    }

    fn lookup(&self, name: *const u8) -> u64 {
        if name.is_null() { return 0; }
        for i in 0..self.count {
            if crate::util::streq(self.syms[i].name, name) {
                return self.syms[i].value;
            }
        }
        0
    }
}

static mut SYMTAB: SymTab = SymTab::new();

pub fn export(name: *const u8, value: u64, weak: bool) {
    unsafe { core::ptr::addr_of_mut!(SYMTAB).as_mut().unwrap().export(name, value, weak); }
}

pub fn lookup(name: *const u8) -> u64 {
    unsafe { core::ptr::addr_of!(SYMTAB).as_ref().unwrap().lookup(name) }
}
