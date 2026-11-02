//! FreeBSD-specific crash context.
//!
//! FreeBSD's `libc` provides a consistent `ucontext_t` and `mcontext_t`
//! (no glibc/musl split like Linux), and `getcontext(3)` is available
//! in the base system, so no hand-rolled assembly is needed.

/// The full context for a FreeBSD crash.
///
/// Note that `ucontext_t::uc_link` is a pointer and thus can't be meaningfully
/// accessed in a process other than the one the `CrashContext` was created in.
/// It is zeroed on construction to avoid leaking addresses across processes.
#[repr(C)]
#[derive(Clone)]
pub struct CrashContext {
    /// The user context at the time of the crash, including general-purpose
    /// registers (`uc_mcontext.mc_*`) and floating-point state
    /// (`uc_mcontext.mc_fpstate`).
    pub context: libc::ucontext_t,
    /// The signal info for the crash.
    pub siginfo: libc::siginfo_t,
    /// The id of the crashing process.
    pub pid: libc::pid_t,
    /// The id of the crashing thread.
    pub tid: libc::pid_t,
}

unsafe impl Send for CrashContext {}

// `getcontext` is not exported by the `libc` crate for FreeBSD.
unsafe extern "C" {
    fn getcontext(ucp: *mut libc::ucontext_t) -> libc::c_int;
}

impl CrashContext {
    /// Captures the current context via `getcontext(3)`.
    pub fn capture() -> Option<Self> {
        let mut context: libc::ucontext_t = unsafe { std::mem::zeroed() };
        // SAFETY: `getcontext` is a well-defined libc function that fills the
        // provided ucontext_t.
        if unsafe { getcontext(&mut context) } != 0 {
            return None;
        }

        Some(Self::from_ucontext(
            context,
            unsafe { std::mem::zeroed() },
            std::process::id() as libc::pid_t,
            thread_self(),
        ))
    }

    /// Creates a `CrashContext` from the pieces a signal handler receives.
    ///
    /// `uc_link` is zeroed since it's a self-referential pointer that's
    /// meaningless in another process.
    pub fn from_signal(
        siginfo: libc::siginfo_t,
        mut context: libc::ucontext_t,
        pid: libc::pid_t,
        tid: libc::pid_t,
    ) -> Self {
        context.uc_link = std::ptr::null_mut();
        Self {
            context,
            siginfo,
            pid,
            tid,
        }
    }

    fn from_ucontext(
        mut context: libc::ucontext_t,
        siginfo: libc::siginfo_t,
        pid: libc::pid_t,
        tid: libc::pid_t,
    ) -> Self {
        context.uc_link = std::ptr::null_mut();
        Self {
            context,
            siginfo,
            pid,
            tid,
        }
    }

    pub fn as_bytes(&self) -> &[u8] {
        // SAFETY: `Self` is `#[repr(C)]` with no padding (all fields are
        // fixed-size integers, pointers, or arrays thereof).
        unsafe {
            let size = std::mem::size_of_val(self);
            let ptr = (self as *const Self).cast();
            std::slice::from_raw_parts(ptr, size)
        }
    }

    pub fn from_bytes(bytes: &[u8]) -> Option<Self> {
        if bytes.len() != std::mem::size_of::<Self>() {
            return None;
        }
        // SAFETY: the size is checked and `Self` is `#[repr(C)]`.
        unsafe { Some(std::ptr::read_unaligned(bytes.as_ptr().cast::<Self>())) }
    }
}

/// Returns the current thread's LWP id via `pthread_self`.
fn thread_self() -> libc::pid_t {
    // SAFETY: `pthread_self` is a well-defined libc function that returns
    // the calling thread's TID on FreeBSD.
    unsafe { libc::pthread_self() as libc::pid_t }
}
