/*
    Copyright (C) 2026 Matej Gomboc <https://github.com/ai-quokka-wannabe/master-control>

    This program is free software: you can redistribute it and/or modify it under the terms of
    the GNU General Public License as published by the Free Software Foundation, either version
    3 of the License, or (at your option) any later version.

    This program is distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY;
    without even the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
    See the GNU General Public License for more details.

    You should have received a copy of the GNU General Public License along with this program.
    If not, see <https://www.gnu.org/licenses/>.
*/

//! The boundary to the wire: Link loaded as the DLL beside this executable, never as a crate.
//!
//! One binary at run time is what makes one-implementation-cannot-drift true, so this module is
//! a *consumer binding*, exactly as every C consumer's `#include` is: the structs and constants
//! of `lnk_protocol.h` and `lnk_client.h` mirrored field for field, pinned by const asserts and
//! by twin tests that parse the pinned submodule's own header text. The load-time guards are
//! the contract's own: `lnkGetClientVTable` refuses any other ABI version, and `vtable_bytes`
//! is checked against this mirror's size before a single call is made.
//!
//! This is the one module allowed `unsafe`: every foreign call and every union read lives here,
//! behind safe wrappers, so the rest of the server never spells the word.

#![allow(unsafe_code)]

use std::ffi::{CString, c_char, c_void};
use std::path::PathBuf;

// ---------------------------------------------------------------------------------------------
// lnk_protocol.h, mirrored. The twin tests below hold every number to the header text itself.
// ---------------------------------------------------------------------------------------------

/// `LNK_PROTOCOL_VERSION` as this server was built. The handshake carries the fingerprint, not
/// this number; the number exists for logs and refusals.
pub const PROTOCOL_VERSION: u32 = 5;

/// `LNK_DEFAULT_PORT`: where Master Control listens when nobody names another port.
pub const DEFAULT_PORT: u16 = 30_702;

/// `LNK_KEEPALIVE_PING_MILLIS`: heard nothing for this long - send a PING.
pub const KEEPALIVE_PING_MILLIS: u64 = 1_000;

/// `LNK_KEEPALIVE_DEAD_MILLIS`: heard nothing for this long - the peer is dead, reap it.
pub const KEEPALIVE_DEAD_MILLIS: u64 = 10_000;

/// `LNK_ACTIONS_REPEAT_TICKS`: how long a connected host's last accepted intent is re-applied
/// when its ACTIONS are merely missing, before zeroed coast.
pub const ACTIONS_REPEAT_TICKS: u32 = 1;

/// `LNK_TICK_STATE_MAX_CREATURES`: the most creatures one TICK_STATE may carry.
pub const TICK_STATE_MAX_CREATURES: u32 = 256;

pub const MSG_HELLO: u8 = 1;
pub const MSG_WELCOME: u8 = 2;
pub const MSG_REZ: u8 = 3;
pub const MSG_TICK_STATE: u8 = 4;
pub const MSG_ACTIONS: u8 = 5;
pub const MSG_EVENT: u8 = 6;
pub const MSG_DEREZ: u8 = 7;
pub const MSG_PING: u8 = 8;
pub const MSG_PONG: u8 = 9;
pub const MSG_BYE: u8 = 10;
pub const MSG_PROPRIOCEPTION: u8 = 11;

pub const ROLE_SPECTATOR: u8 = 1;
pub const ROLE_CREATURE_HOST: u8 = 2;

pub const EVENT_VOCALISATION: u8 = 1;

/// `LNK_REZ_MAX_*`: the three caps of the one variable-size client input. The wire judges them
/// before any copy; this side restates them so a roster can size itself without asking.
pub const REZ_MAX_VERTICES: u32 = 1_024;
pub const REZ_MAX_TRIANGLES: u32 = 2_048;
pub const REZ_MAX_MATERIALS: u32 = 16;

/// `LNK_CONTACTS_MAX`: the most contacts the owner's letter carries, and so the most a body
/// may declare.
pub const CONTACTS_MAX: u32 = 16;

/// `LnkContact`: one contact a body felt this tick - where, and the impulse delivered there.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Contact {
    pub position: [f32; 3],
    pub impulse: [f32; 3],
}

/// `LnkProprioception`: the owner's letter - the body's feel this tick, followed on the wire
/// by `contact_count` [`Contact`] rows. Server to the owning host only.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Proprioception {
    pub tick: u64,
    pub creature_id: u32,
    pub grounded: u8,
    pub reserved0: [u8; 3],
    pub specific_force: [f32; 3],
    pub contact_count: u32,
}

/// `LnkWorldDefinition`: what the simulated world is made of, the fields both ends must agree
/// on before a position means the same thing twice. The fingerprint over it is the DLL's to
/// compute ([`LinkDll::world_fingerprint`]) - never this side's.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct WorldDefinition {
    pub floor_cells: u32,
    pub floor_cell_size: f32,
    pub floor_height: f32,
    pub relief_amplitude: f32,
    pub relief_wavelength: f32,
    pub relief_octaves: u32,
    pub relief_terraces: u32,
    pub relief_seed: u32,
    pub dt_seconds: f32,
    pub body_half_height: f32,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Hello {
    pub protocol_version: u32,
    pub fingerprint: [u8; 32],
    pub role: u8,
    pub reserved0: [u8; 3],
    pub world_fingerprint: u64,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Welcome {
    pub current_tick: u64,
    pub nominal_dt_seconds: f32,
    pub client_id: u32,
    pub world_fingerprint: u64,
}

/// `LnkRez`: a creature's bounds and the counts of the rows that follow it on the wire.
#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Rez {
    pub creature_id: u32,
    pub max_forward_speed: f32,
    pub max_turn_rate: f32,
    pub max_vocalisation_strength: f32,
    pub max_contact_count: u32,
    pub vertex_count: u32,
    pub triangle_count: u32,
    pub material_count: u32,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RezVertex {
    pub position: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct RezTriangle {
    pub vertices: [u32; 3],
    pub material: u32,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct RezMaterial {
    pub colour: [f32; 3],
    pub index_of_refraction: f32,
    pub emission: [f32; 3],
    pub transmission: f32,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct CreatureState {
    pub creature_id: u32,
    pub position: [f32; 3],
    pub yaw: f32,
    pub velocity: [f32; 3],
    pub yaw_rate: f32,
    pub vocalisation: f32,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct TickStateHeader {
    pub tick: u64,
    pub creature_count: u32,
    pub reserved0: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Actions {
    pub tick: u64,
    pub creature_id: u32,
    pub desired_forward_speed: f32,
    pub desired_turn_rate: f32,
    pub vocalisation_strength: f32,
    pub previous_forward_speed: f32,
    pub previous_turn_rate: f32,
    pub previous_vocalisation: f32,
    pub reserved0: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Event {
    pub tick: u64,
    pub position: [f32; 3],
    pub strength: f32,
    pub creature_id: u32,
    pub kind: u8,
    pub reserved0: [u8; 3],
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Derez {
    pub tick: u64,
    pub creature_id: u32,
    pub reserved0: [u8; 4],
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Ping {
    pub nonce: u64,
}

#[repr(C)]
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Pong {
    pub nonce: u64,
}

const _: () = assert!(size_of::<WorldDefinition>() == 40);
const _: () = assert!(size_of::<Hello>() == 48);
const _: () = assert!(size_of::<Welcome>() == 24);
const _: () = assert!(size_of::<Rez>() == 32);
const _: () = assert!(size_of::<RezVertex>() == 12);
const _: () = assert!(size_of::<RezTriangle>() == 16);
const _: () = assert!(size_of::<RezMaterial>() == 32);
const _: () = assert!(size_of::<Contact>() == 24);
const _: () = assert!(size_of::<Proprioception>() == 32);
const _: () = assert!(size_of::<CreatureState>() == 40);
const _: () = assert!(size_of::<TickStateHeader>() == 16);
const _: () = assert!(size_of::<Actions>() == 40);
const _: () = assert!(size_of::<Event>() == 32);
const _: () = assert!(size_of::<Derez>() == 16);

// ---------------------------------------------------------------------------------------------
// lnk_client.h, mirrored: statuses, views, the vtable. The order of vtable fields is the ABI.
// ---------------------------------------------------------------------------------------------

/// `LNK_CLIENT_ABI_VERSION` this binding was written against; the export refuses any other.
pub const CLIENT_ABI_VERSION: u32 = 6;

pub type LnkStatus = i32;

pub const LNK_OK: LnkStatus = 0;
pub const LNK_NOTHING_YET: LnkStatus = 1;
pub const LNK_REFUSED: LnkStatus = 2;
pub const LNK_HANDSHAKE_TIMED_OUT: LnkStatus = 3;
pub const LNK_PEER_CLOSED: LnkStatus = 4;
pub const LNK_FRAME_REFUSED: LnkStatus = 5;
pub const LNK_GARBLED: LnkStatus = 6;
pub const LNK_IO: LnkStatus = 7;
pub const LNK_BAD_ARGUMENT: LnkStatus = 8;
pub const LNK_PANIC: LnkStatus = 9;

/// Opaque: a connection handle the DLL owns. Only the vtable relates it to anything.
#[repr(C)]
pub struct LnkClient {
    _opaque: [u8; 0],
}

/// Opaque: a listening handle the DLL owns.
#[repr(C)]
pub struct LnkServer {
    _opaque: [u8; 0],
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct TickStateView {
    pub header: TickStateHeader,
    pub states: *const CreatureState,
}

/// `LnkRezView`: the header by value, the rows borrowed until the next poll.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct RezView {
    pub rez: Rez,
    pub vertices: *const RezVertex,
    pub triangles: *const RezTriangle,
    pub materials: *const RezMaterial,
}

/// `LnkProprioceptionView`: the header by value, the contacts borrowed until the next poll.
#[repr(C)]
#[derive(Clone, Copy)]
pub struct ProprioceptionView {
    pub proprioception: Proprioception,
    pub contacts: *const Contact,
}

/// The union behind `LnkMessageView.as`. Reading the member the type byte names is the
/// contract; [`MessageView::message`] is the one place that read happens.
#[repr(C)]
#[derive(Clone, Copy)]
pub union MessageViewPayload {
    pub welcome: Welcome,
    pub tick_state: TickStateView,
    pub event: Event,
    pub derez: Derez,
    pub ping: Ping,
    pub pong: Pong,
    pub hello: Hello,
    pub actions: Actions,
    pub rez: RezView,
    pub proprioception: ProprioceptionView,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct MessageView {
    pub message_type: u8,
    pub reserved0: [u8; 7],
    pub payload: MessageViewPayload,
}

/// A received message, owned: the union resolved by its type byte, TICK_STATE's borrowed rows
/// copied out before the next poll can replace them.
#[derive(Clone, PartialEq, Debug)]
pub enum Message {
    Hello(Hello),
    Welcome(Welcome),
    TickState {
        header: TickStateHeader,
        states: Vec<CreatureState>,
    },
    Actions(Actions),
    /// A body, rows copied out - the wire already judged counts, indices and finiteness.
    Rez {
        header: Rez,
        vertices: Vec<RezVertex>,
        triangles: Vec<RezTriangle>,
        materials: Vec<RezMaterial>,
    },
    Event(Event),
    Derez(Derez),
    /// The owner's letter, contacts copied out.
    Proprioception {
        header: Proprioception,
        contacts: Vec<Contact>,
    },
    Ping(Ping),
    Pong(Pong),
    Bye,
    /// A type byte this mirror does not know. The wire refuses unknown types itself, so seeing
    /// one here means the mirror is older than the library - worth a log, never a crash.
    Unknown(u8),
}

/// Rows copied out of a borrowed view: a zero count never touches the pointer.
///
/// # Safety
///
/// `pointer` must point at `count` rows the library wrote, which is the view's contract.
unsafe fn rows<T: Copy>(pointer: *const T, count: u32) -> Vec<T> {
    if count == 0 || pointer.is_null() {
        Vec::new()
    } else {
        // SAFETY: delegated to the caller, per the contract above.
        unsafe { std::slice::from_raw_parts(pointer, count as usize) }.to_vec()
    }
}

impl MessageView {
    /// The union resolved by the contract's own rule: read the member the type byte names.
    fn message(&self) -> Message {
        // SAFETY: the library wrote `payload` as the member `message_type` names, which is the
        // documented contract of poll(); every arm reads exactly that member.
        unsafe {
            match self.message_type {
                MSG_HELLO => Message::Hello(self.payload.hello),
                MSG_WELCOME => Message::Welcome(self.payload.welcome),
                MSG_TICK_STATE => {
                    let view = self.payload.tick_state;
                    let count = view.header.creature_count as usize;
                    // The library validated the count against the cap before writing the view;
                    // an empty snapshot hands a null pointer with a zero count.
                    let states = if count == 0 || view.states.is_null() {
                        Vec::new()
                    } else {
                        std::slice::from_raw_parts(view.states, count).to_vec()
                    };
                    Message::TickState {
                        header: view.header,
                        states,
                    }
                }
                MSG_ACTIONS => Message::Actions(self.payload.actions),
                MSG_REZ => {
                    let view = self.payload.rez;
                    Message::Rez {
                        header: view.rez,
                        vertices: rows(view.vertices, view.rez.vertex_count),
                        triangles: rows(view.triangles, view.rez.triangle_count),
                        materials: rows(view.materials, view.rez.material_count),
                    }
                }
                MSG_EVENT => Message::Event(self.payload.event),
                MSG_PROPRIOCEPTION => {
                    let view = self.payload.proprioception;
                    Message::Proprioception {
                        header: view.proprioception,
                        contacts: rows(view.contacts, view.proprioception.contact_count),
                    }
                }
                MSG_DEREZ => Message::Derez(self.payload.derez),
                MSG_PING => Message::Ping(self.payload.ping),
                MSG_PONG => Message::Pong(self.payload.pong),
                MSG_BYE => Message::Bye,
                other => Message::Unknown(other),
            }
        }
    }
}

/// `LnkClientVTable`, field for field and in the header's exact order - the order is the ABI,
/// and `vtable_bytes` is checked against this mirror's size before anything is called.
#[repr(C)]
pub struct LnkClientVTable {
    pub vtable_bytes: u32,
    pub abi_version: u32,
    pub protocol_version: extern "C" fn() -> u32,
    pub protocol_fingerprint: extern "C" fn(out_fingerprint: *mut u8),
    pub world_fingerprint: extern "C" fn(definition: *const WorldDefinition) -> u64,
    pub connect: extern "C" fn(
        address_utf8: *const c_char,
        role: u8,
        world_fingerprint: u64,
        timeout_milliseconds: u32,
        out_welcome: *mut Welcome,
        out_status: *mut LnkStatus,
        out_detail_utf8: *mut c_char,
        detail_capacity_bytes: u32,
    ) -> *mut LnkClient,
    pub poll: extern "C" fn(client: *mut LnkClient, out_message: *mut MessageView) -> LnkStatus,
    pub send_actions: extern "C" fn(client: *mut LnkClient, actions: *const Actions) -> LnkStatus,
    pub send_rez: extern "C" fn(
        client: *mut LnkClient,
        rez: *const Rez,
        vertices: *const RezVertex,
        triangles: *const RezTriangle,
        materials: *const RezMaterial,
    ) -> LnkStatus,
    pub send_ping: extern "C" fn(client: *mut LnkClient, nonce: u64) -> LnkStatus,
    pub send_pong: extern "C" fn(client: *mut LnkClient, nonce: u64) -> LnkStatus,
    pub flush: extern "C" fn(client: *mut LnkClient, out_everything_left: *mut u8) -> LnkStatus,
    pub close: extern "C" fn(client: *mut LnkClient),
    pub listen: extern "C" fn(
        port: u16,
        world_fingerprint: u64,
        out_status: *mut LnkStatus,
        out_detail_utf8: *mut c_char,
        detail_capacity_bytes: u32,
    ) -> *mut LnkServer,
    pub server_port: extern "C" fn(server: *mut LnkServer) -> u16,
    pub accept: extern "C" fn(
        server: *mut LnkServer,
        timeout_milliseconds: u32,
        out_hello: *mut Hello,
        out_status: *mut LnkStatus,
        out_detail_utf8: *mut c_char,
        detail_capacity_bytes: u32,
    ) -> *mut LnkClient,
    pub send_welcome:
        extern "C" fn(connection: *mut LnkClient, welcome: *const Welcome) -> LnkStatus,
    pub send_tick_state: extern "C" fn(
        connection: *mut LnkClient,
        header: *const TickStateHeader,
        states: *const CreatureState,
    ) -> LnkStatus,
    pub send_event: extern "C" fn(connection: *mut LnkClient, event: *const Event) -> LnkStatus,
    pub send_derez: extern "C" fn(connection: *mut LnkClient, derez: *const Derez) -> LnkStatus,
    pub send_proprioception: extern "C" fn(
        connection: *mut LnkClient,
        proprioception: *const Proprioception,
        contacts: *const Contact,
    ) -> LnkStatus,
    pub close_server: extern "C" fn(server: *mut LnkServer),
    pub record_open: extern "C" fn(
        path_utf8: *const c_char,
        world_fingerprint: u64,
        start_tick: u64,
        nominal_dt_seconds: f32,
        start_unix_seconds: u64,
        out_status: *mut LnkStatus,
        out_detail_utf8: *mut c_char,
        detail_capacity_bytes: u32,
    ) -> *mut LnkClient,
    pub replay_open: extern "C" fn(
        path_utf8: *const c_char,
        world_fingerprint: u64,
        out_welcome: *mut Welcome,
        out_status: *mut LnkStatus,
        out_detail_utf8: *mut c_char,
        detail_capacity_bytes: u32,
    ) -> *mut LnkClient,
}

type GetClientVTableFn = extern "C" fn(abi_version: u32) -> *const LnkClientVTable;

// ---------------------------------------------------------------------------------------------
// The loader: the residence rule in a few dozen lines of extern declarations, per the no-crates
// rule. The module handle is deliberately never freed - the wire lives as long as the world.
// ---------------------------------------------------------------------------------------------

#[cfg(target_os = "windows")]
mod os {
    use super::c_void;

    unsafe extern "system" {
        fn LoadLibraryW(file_name: *const u16) -> *mut c_void;
        fn GetProcAddress(module: *mut c_void, name: *const super::c_char) -> *mut c_void;
    }

    pub fn open(path: &std::path::Path) -> Result<*mut c_void, String> {
        let wide: Vec<u16> = path
            .as_os_str()
            .to_string_lossy()
            .encode_utf16()
            .chain(std::iter::once(0))
            .collect();
        // SAFETY: `wide` is NUL-terminated and outlives the call.
        let module = unsafe { LoadLibraryW(wide.as_ptr()) };
        if module.is_null() {
            Err(format!("the library at {} would not load", path.display()))
        } else {
            Ok(module)
        }
    }

    pub fn symbol(module: *mut c_void, name: &std::ffi::CStr) -> *mut c_void {
        // SAFETY: `module` came from open() and `name` is NUL-terminated.
        unsafe { GetProcAddress(module, name.as_ptr()) }
    }

    pub const LIBRARY_NAME: &str = "link.dll";
}

#[cfg(not(target_os = "windows"))]
mod os {
    use super::c_void;

    unsafe extern "C" {
        fn dlopen(file_name: *const super::c_char, flags: i32) -> *mut c_void;
        fn dlsym(module: *mut c_void, name: *const super::c_char) -> *mut c_void;
    }

    const RTLD_NOW: i32 = 2;

    pub fn open(path: &std::path::Path) -> Result<*mut c_void, String> {
        let c_path = std::ffi::CString::new(path.as_os_str().to_string_lossy().as_bytes())
            .map_err(|_| "the library path holds a NUL".to_string())?;
        // SAFETY: `c_path` is NUL-terminated and outlives the call. RTLD_LOCAL is the default.
        let module = unsafe { dlopen(c_path.as_ptr(), RTLD_NOW) };
        if module.is_null() {
            Err(format!("the library at {} would not load", path.display()))
        } else {
            Ok(module)
        }
    }

    pub fn symbol(module: *mut c_void, name: &std::ffi::CStr) -> *mut c_void {
        // SAFETY: `module` came from open() and `name` is NUL-terminated.
        unsafe { dlsym(module, name.as_ptr()) }
    }

    pub const LIBRARY_NAME: &str = "liblink.so";
}

/// The loaded wire: the vtable, version-refused and size-checked, from the library beside this
/// executable - the residence rule, no path flag, no search order.
pub struct LinkDll {
    vtable: &'static LnkClientVTable,
}

impl LinkDll {
    /// Loads `link.dll`/`liblink.so` from beside the running executable and acquires the
    /// vtable. Refuses loudly on a missing library, a missing export, a refused ABI version or
    /// a vtable whose size disagrees with this mirror - each of which means a stale artefact,
    /// and a stale wire must never look like a working one.
    pub fn beside_executable() -> Result<LinkDll, String> {
        let executable = std::env::current_exe()
            .map_err(|error| format!("where is this executable? {error}"))?;
        let directory = executable.parent().map(PathBuf::from).unwrap_or_default();
        let path = directory.join(os::LIBRARY_NAME);

        let module = os::open(&path)?;
        let symbol_name = CString::new("lnkGetClientVTable").expect("a literal has no NUL");
        let raw = os::symbol(module, &symbol_name);
        if raw.is_null() {
            return Err(format!(
                "{} exports no lnkGetClientVTable - not the wire of the Grid",
                path.display()
            ));
        }
        // SAFETY: the export's type is the header's contract; the transmute is the FFI cast.
        let get_vtable: GetClientVTableFn =
            unsafe { std::mem::transmute::<*mut c_void, GetClientVTableFn>(raw) };

        let table = get_vtable(CLIENT_ABI_VERSION);
        if table.is_null() {
            return Err(format!(
                "the wire at {} refuses client ABI version {CLIENT_ABI_VERSION} - rebuild one side until the versions agree",
                path.display()
            ));
        }
        // SAFETY: a non-null return is the library's static table, alive for the process.
        let vtable = unsafe { &*table };
        if vtable.vtable_bytes as usize != size_of::<LnkClientVTable>() {
            return Err(format!(
                "the wire's vtable is {} bytes where this build expects {} - a stale artefact on one side",
                vtable.vtable_bytes,
                size_of::<LnkClientVTable>()
            ));
        }
        Ok(LinkDll { vtable })
    }

    #[must_use]
    pub fn protocol_version(&self) -> u32 {
        (self.vtable.protocol_version)()
    }

    #[must_use]
    pub fn vtable(&self) -> &'static LnkClientVTable {
        self.vtable
    }
}

// ---------------------------------------------------------------------------------------------
// Safe wrappers around the server half: the rest of the process speaks these, never a pointer.
// ---------------------------------------------------------------------------------------------

/// A listening socket the DLL owns, closed on drop.
pub struct Listener {
    vtable: &'static LnkClientVTable,
    server: *mut LnkServer,
}

/// One accepted, welcomed-or-not conversation, closed on drop.
pub struct Connection {
    vtable: &'static LnkClientVTable,
    client: *mut LnkClient,
}

// SAFETY: the handles are heap state the library owns with no thread affinity - sockets and
// buffers, no thread-locals - and the header's contract is calls from one thread at a time,
// which exclusive ownership and `&mut self` already enforce. Send moves that whole ownership;
// Sync stays unimplemented, deliberately, so two threads can never speak into one handle.
unsafe impl Send for Listener {}
// SAFETY: as for Listener.
unsafe impl Send for Connection {}

impl LinkDll {
    /// The client half's handshake, wrapped for the tests that dial the world this server
    /// stands up. Master Control itself never places an outbound call.
    /// The fingerprint over a world definition, by the one implementation there is.
    pub fn world_fingerprint(&self, definition: &WorldDefinition) -> u64 {
        (self.vtable.world_fingerprint)(definition)
    }

    pub fn connect(
        &self,
        address: &str,
        role: u8,
        world_fingerprint: u64,
        timeout_ms: u32,
    ) -> Result<(Connection, Welcome), String> {
        let c_address = CString::new(address).map_err(|_| "the address holds a NUL".to_string())?;
        let mut welcome = Welcome {
            current_tick: 0,
            nominal_dt_seconds: 0.0,
            client_id: 0,
            world_fingerprint: 0,
        };
        let mut status: LnkStatus = LNK_PANIC;
        let mut detail = [0u8; 256];
        let client = (self.vtable.connect)(
            c_address.as_ptr(),
            role,
            world_fingerprint,
            timeout_ms,
            &raw mut welcome,
            &raw mut status,
            detail.as_mut_ptr().cast::<c_char>(),
            detail.len() as u32,
        );
        if client.is_null() {
            Err(format!(
                "no Master Control at {address}: {}",
                detail_text(&detail, status)
            ))
        } else {
            Ok((
                Connection {
                    vtable: self.vtable,
                    client,
                },
                welcome,
            ))
        }
    }

    /// Open a Disk - a client whose socket is a file - as a server-held end: everything this
    /// world tells its citizens is told to it too, and a replay viewer is a spectator that
    /// opened it. The handle is a [`Connection`] like any citizen's, closed (BYE written) on drop.
    pub fn record_open(
        &self,
        path: &std::path::Path,
        world_fingerprint: u64,
        start_tick: u64,
        nominal_dt_seconds: f32,
        start_unix_seconds: u64,
    ) -> Result<Connection, String> {
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| "the Disk path holds a NUL".to_string())?;
        let mut status: LnkStatus = LNK_PANIC;
        let mut detail = [0u8; 256];
        let client = (self.vtable.record_open)(
            c_path.as_ptr(),
            world_fingerprint,
            start_tick,
            nominal_dt_seconds,
            start_unix_seconds,
            &raw mut status,
            detail.as_mut_ptr().cast::<c_char>(),
            detail.len() as u32,
        );
        if client.is_null() {
            Err(format!(
                "could not open the Disk at {}: {}",
                path.display(),
                detail_text(&detail, status)
            ))
        } else {
            Ok(Connection {
                vtable: self.vtable,
                client,
            })
        }
    }

    /// Open a Disk for reading, as a client-held end: the WELCOME is the Disk's own header.
    pub fn replay_open(
        &self,
        path: &std::path::Path,
        world_fingerprint: u64,
    ) -> Result<(Connection, Welcome), String> {
        let c_path = CString::new(path.to_string_lossy().as_bytes())
            .map_err(|_| "the Disk path holds a NUL".to_string())?;
        let mut welcome = Welcome {
            current_tick: 0,
            nominal_dt_seconds: 0.0,
            client_id: 0,
            world_fingerprint: 0,
        };
        let mut status: LnkStatus = LNK_PANIC;
        let mut detail = [0u8; 256];
        let client = (self.vtable.replay_open)(
            c_path.as_ptr(),
            world_fingerprint,
            &raw mut welcome,
            &raw mut status,
            detail.as_mut_ptr().cast::<c_char>(),
            detail.len() as u32,
        );
        if client.is_null() {
            Err(format!(
                "could not open the Disk at {}: {}",
                path.display(),
                detail_text(&detail, status)
            ))
        } else {
            Ok((
                Connection {
                    vtable: self.vtable,
                    client,
                },
                welcome,
            ))
        }
    }

    /// Listen on the port (0 asks the operating system; [`Listener::port`] answers which) as
    /// the world the fingerprint names: every HELLO is judged against it at the door.
    pub fn listen(&self, port: u16, world_fingerprint: u64) -> Result<Listener, String> {
        let mut status: LnkStatus = LNK_PANIC;
        let mut detail = [0u8; 256];
        let server = (self.vtable.listen)(
            port,
            world_fingerprint,
            &raw mut status,
            detail.as_mut_ptr().cast::<c_char>(),
            detail.len() as u32,
        );
        if server.is_null() {
            Err(format!(
                "could not listen on port {port}: {}",
                detail_text(&detail, status)
            ))
        } else {
            Ok(Listener {
                vtable: self.vtable,
                server,
            })
        }
    }
}

impl Listener {
    #[must_use]
    pub fn port(&self) -> u16 {
        (self.vtable.server_port)(self.server)
    }

    /// One knock if somebody knocked: the whole handshake walked, `None` when nobody waits.
    /// A refused handshake is a log line, not an error - the refusal already went to the peer.
    pub fn accept(&self, handshake_timeout_ms: u32) -> Option<(Connection, Hello)> {
        let mut hello = Hello {
            protocol_version: 0,
            fingerprint: [0; 32],
            role: 0,
            reserved0: [0; 3],
            world_fingerprint: 0,
        };
        let mut status: LnkStatus = LNK_PANIC;
        let mut detail = [0u8; 256];
        let client = (self.vtable.accept)(
            self.server,
            handshake_timeout_ms,
            &raw mut hello,
            &raw mut status,
            detail.as_mut_ptr().cast::<c_char>(),
            detail.len() as u32,
        );
        if client.is_null() {
            if status != LNK_NOTHING_YET {
                log_info(&format!(
                    "a knock came to nothing: {}",
                    detail_text(&detail, status)
                ));
            }
            None
        } else {
            Some((
                Connection {
                    vtable: self.vtable,
                    client,
                },
                hello,
            ))
        }
    }
}

impl Drop for Listener {
    fn drop(&mut self) {
        (self.vtable.close_server)(self.server);
    }
}

impl Connection {
    pub fn send_actions(&mut self, actions: &Actions) -> LnkStatus {
        (self.vtable.send_actions)(self.client, actions)
    }

    /// A body, rows by borrow; the library copies them before returning, and judges the counts
    /// against the caps before reading a single row.
    pub fn send_rez(
        &mut self,
        header: &Rez,
        vertices: &[RezVertex],
        triangles: &[RezTriangle],
        materials: &[RezMaterial],
    ) -> LnkStatus {
        if vertices.len() != header.vertex_count as usize
            || triangles.len() != header.triangle_count as usize
            || materials.len() != header.material_count as usize
        {
            return LNK_BAD_ARGUMENT;
        }
        (self.vtable.send_rez)(
            self.client,
            header,
            vertices.as_ptr(),
            triangles.as_ptr(),
            materials.as_ptr(),
        )
    }

    pub fn send_welcome(&mut self, welcome: &Welcome) -> LnkStatus {
        (self.vtable.send_welcome)(self.client, welcome)
    }

    pub fn send_tick_state(
        &mut self,
        header: &TickStateHeader,
        states: &[CreatureState],
    ) -> LnkStatus {
        (self.vtable.send_tick_state)(
            self.client,
            header,
            if states.is_empty() {
                std::ptr::null()
            } else {
                states.as_ptr()
            },
        )
    }

    pub fn send_event(&mut self, event: &Event) -> LnkStatus {
        (self.vtable.send_event)(self.client, event)
    }

    pub fn send_derez(&mut self, derez: &Derez) -> LnkStatus {
        (self.vtable.send_derez)(self.client, derez)
    }

    /// The owner's letter, contacts by borrow; the library copies them and judges the count
    /// against the cap before reading a row.
    pub fn send_proprioception(
        &mut self,
        header: &Proprioception,
        contacts: &[Contact],
    ) -> LnkStatus {
        if contacts.len() != header.contact_count as usize {
            return LNK_BAD_ARGUMENT;
        }
        (self.vtable.send_proprioception)(self.client, header, contacts.as_ptr())
    }

    pub fn send_ping(&mut self, nonce: u64) -> LnkStatus {
        (self.vtable.send_ping)(self.client, nonce)
    }

    pub fn send_pong(&mut self, nonce: u64) -> LnkStatus {
        (self.vtable.send_pong)(self.client, nonce)
    }

    /// One complete message if the socket holds one. `Ok(None)` when it does not yet; any error
    /// status ends the connection's useful life, and the caller drops it.
    pub fn poll(&mut self) -> Result<Option<Message>, LnkStatus> {
        let mut view = MessageView {
            message_type: 0,
            reserved0: [0; 7],
            payload: MessageViewPayload {
                ping: Ping { nonce: 0 },
            },
        };
        match (self.vtable.poll)(self.client, &raw mut view) {
            LNK_OK => Ok(Some(view.message())),
            LNK_NOTHING_YET => Ok(None),
            other => Err(other),
        }
    }

    /// Push everything staged; `Ok(true)` when the buffer emptied. An error status is the
    /// connection's end - including the wire's own write-buffer high-water verdict.
    pub fn flush(&mut self) -> Result<bool, LnkStatus> {
        let mut everything_left: u8 = 0;
        match (self.vtable.flush)(self.client, &raw mut everything_left) {
            LNK_OK => Ok(everything_left == 1),
            other => Err(other),
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        (self.vtable.close)(self.client);
    }
}

fn detail_text(detail: &[u8; 256], status: LnkStatus) -> String {
    let end = detail
        .iter()
        .position(|byte| *byte == 0)
        .unwrap_or(detail.len());
    let words = String::from_utf8_lossy(&detail[..end]);
    if words.is_empty() {
        format!("status {status}")
    } else {
        words.to_string()
    }
}

fn log_info(message: &str) {
    println!("[INFO] {message}");
}

// ---------------------------------------------------------------------------------------------
// Twin tests: every mirrored number held to the pinned submodule's own header text - the same
// mechanism link's protocol.rs uses on itself, applied from the consumer's side.
// ---------------------------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    const PROTOCOL_HEADER: &str = include_str!("../external/link/include/lnk/lnk_protocol.h");
    const CLIENT_HEADER: &str = include_str!("../external/link/include/lnk/lnk_client.h");

    #[test]
    fn the_mirrored_constants_are_the_headers() {
        for (name, value) in [
            ("LNK_PROTOCOL_VERSION", u64::from(PROTOCOL_VERSION)),
            ("LNK_DEFAULT_PORT", u64::from(DEFAULT_PORT)),
            ("LNK_KEEPALIVE_PING_MILLIS", KEEPALIVE_PING_MILLIS),
            ("LNK_KEEPALIVE_DEAD_MILLIS", KEEPALIVE_DEAD_MILLIS),
            ("LNK_ACTIONS_REPEAT_TICKS", u64::from(ACTIONS_REPEAT_TICKS)),
            (
                "LNK_TICK_STATE_MAX_CREATURES",
                u64::from(TICK_STATE_MAX_CREATURES),
            ),
            ("LNK_MSG_HELLO", u64::from(MSG_HELLO)),
            ("LNK_MSG_REZ", u64::from(MSG_REZ)),
            ("LNK_MSG_PROPRIOCEPTION", u64::from(MSG_PROPRIOCEPTION)),
            ("LNK_CONTACTS_MAX", u64::from(CONTACTS_MAX)),
            ("LNK_MSG_BYE", u64::from(MSG_BYE)),
            ("LNK_REZ_MAX_VERTICES", u64::from(REZ_MAX_VERTICES)),
            ("LNK_REZ_MAX_TRIANGLES", u64::from(REZ_MAX_TRIANGLES)),
            ("LNK_REZ_MAX_MATERIALS", u64::from(REZ_MAX_MATERIALS)),
            ("LNK_ROLE_SPECTATOR", u64::from(ROLE_SPECTATOR)),
            ("LNK_ROLE_CREATURE_HOST", u64::from(ROLE_CREATURE_HOST)),
            ("LNK_EVENT_VOCALISATION", u64::from(EVENT_VOCALISATION)),
        ] {
            assert!(
                PROTOCOL_HEADER.contains(&format!("#define {name} {value}u")),
                "{name} drifted from lnk_protocol.h"
            );
        }
        assert!(
            CLIENT_HEADER.contains(&format!(
                "#define LNK_CLIENT_ABI_VERSION {CLIENT_ABI_VERSION}u"
            )),
            "LNK_CLIENT_ABI_VERSION drifted from lnk_client.h"
        );
    }

    #[test]
    fn the_statuses_are_the_headers() {
        for (name, value) in [
            ("LNK_OK", LNK_OK),
            ("LNK_NOTHING_YET", LNK_NOTHING_YET),
            ("LNK_REFUSED", LNK_REFUSED),
            ("LNK_HANDSHAKE_TIMED_OUT", LNK_HANDSHAKE_TIMED_OUT),
            ("LNK_PEER_CLOSED", LNK_PEER_CLOSED),
            ("LNK_FRAME_REFUSED", LNK_FRAME_REFUSED),
            ("LNK_GARBLED", LNK_GARBLED),
            ("LNK_IO", LNK_IO),
            ("LNK_BAD_ARGUMENT", LNK_BAD_ARGUMENT),
            ("LNK_PANIC", LNK_PANIC),
        ] {
            assert!(
                CLIENT_HEADER.contains(&format!("#define {name} {value}")),
                "{name} drifted from lnk_client.h"
            );
        }
    }

    #[test]
    fn the_wire_beside_the_test_executable_loads_and_agrees() {
        let wire = LinkDll::beside_executable()
            .expect("the build script put the wire beside this executable");
        assert_eq!(
            wire.protocol_version(),
            PROTOCOL_VERSION,
            "the loaded wire speaks a different protocol than this mirror"
        );

        let mut fingerprint = [0u8; 32];
        (wire.vtable().protocol_fingerprint)(fingerprint.as_mut_ptr());
        assert_ne!(
            fingerprint, [0u8; 32],
            "a fingerprint of all zeroes is no fingerprint"
        );
    }
}
