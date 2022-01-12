use byteorder::LE;
use std::marker::PhantomData;
use std::sync::Arc;
use fugue::ir::{
    Address,
    IntoAddress,
    space::AddressSpace,
    };
use fuguex::concrete::hooks::{ClonableHookConcrete, HookConcrete};
use fuguex::hooks::types::{HookAction, HookOutcome, Error};
use fuguex::state::{
    State,
    pcode::PCodeState, StateOps};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WatchpointKind {
    Read,
    ReadWrite,
    Write,
}

impl WatchpointKind {
    pub fn is_read(&self) -> bool {
        matches!(self, WatchpointKind::Read)
    }

    pub fn is_write(&self) -> bool {
        matches!(self, WatchpointKind::Write)
    }
}

// impl From<WatchpointKind> for ObservationKind {
//     fn from(kind: WatchpointKind) -> Self {
//         match kind {
//             WatchpointKind::Read => MemoryRead,
//             WatchpointKind::Write => MemoryWrite,
//             WatchpointKind::ReadWrite => MemoryRead | MemoryWrite,
//         }
//     }
// }

pub struct Watchpoint<S, E> {
    observer: Arc<dyn Fn(Address, &[u8], &mut PCodeState<u8, LE>, WatchpointKind) -> Result<(), E> + Send + Sync>,
    address: Address,
    state: PhantomData<S>,
}

impl <S, E> Clone for Watchpoint<S, E> {
    fn clone(&self) -> Self {
        Watchpoint {
            observer: self.observer.clone(),
            address: self.address,
            state: PhantomData,
        }
    }
}

impl<S, E> Watchpoint<S, E>
where S: State,
      E: std::error::Error + Send + Sync + 'static {
    pub fn new_unboxed<A, F>(space: Arc<AddressSpace> ,address: A, _kind: WatchpointKind, observer: F) -> Self 
    where A: IntoAddress, 
            S: State,
            E: std::error::Error + Send + Sync + 'static, 
            F: Fn(Address, &[u8], &mut PCodeState<u8, LE>, WatchpointKind) -> Result<(), E> + Send + Sync + 'static {
        Self { 
            observer: Arc::new(observer),
            address: address.into_address(&space.clone()),
            state: PhantomData,
        }
    }
}

impl<S: 'static,E> HookConcrete for Watchpoint<S, E>
where S: State + StateOps,
        E: std::error::Error + Send + Sync + 'static,
{
    type State = PCodeState<u8, LE>;
    type Error = E;
    type Outcome = String;

    fn hook_memory_read(
        &mut self,
        state: &mut Self::State,
        address: &Address,
        _size: usize,
    ) -> Result<HookOutcome<HookAction<Self::Outcome>>, Error<Self::Error>> {
        if address == &self.address {
            // Get the value at the address
            let mut buf = [0u8; 8];
            state.get_values(address, &mut buf).unwrap();
            // invoke the observer callback
            (*self.observer)(
                address.clone(),
                &buf,
                state,
                WatchpointKind::Read,
            ).unwrap();
            Ok(HookAction::Pass.into())
        } else {
            Ok(HookAction::Pass.into())
        }
    }

    fn hook_memory_write(
        &mut self,
        state: &mut Self::State,
        address: &Address,
        _size: usize,
        value: &[u8],
    ) -> Result<HookOutcome<HookAction<Self::Outcome>>, Error<Self::Error>> {
        if address == &self.address {
            // invoke the observer callback
            (*self.observer)(
                address.clone(),
                value,
                state,
                WatchpointKind::Write,
            ).unwrap();
            Ok(HookAction::Pass.into())
        } else {
            Ok(HookAction::Pass.into())
        }
    }
}
impl<S: 'static, E> ClonableHookConcrete for Watchpoint<S, E>
where S: State + StateOps,
      E: std::error::Error + Send + Sync + 'static{
}