
use thiserror::Error;
use fugue::ir::{
    Address,
    il::pcode::{PCode}
};
// use fugue::bytes::{ByteCast, BE, LE};
use fuguex::concrete::hooks::{ClonableHookConcrete, HookConcrete};
use fuguex::hooks::types::{HookStepAction, HookOutcome, Error as HookError};
use fuguex::state::{
    AsState,
    pcode::PCodeState, StateOps
};
use fuguex::machine::StepState;
use fugue::bytes::{Order};
use std::fs::File;
use std::path::Path;
use std::io::Write;
// use muexe_taint::state::PCodeTaint;
// use muexe_core_prelude::observers::{TraceHook};
use protobuf::Message;
use crate::utils::tbb;

use std::marker::PhantomData;
use std::mem::take;
use std::sync::Arc;
use parking_lot::Mutex;

#[derive(Debug, Error)]
pub enum TraceCollectorError {
    #[error("TraceCollector Error")]
    DefaultError(),
}


#[derive(Clone, Default)]
pub struct TraceCollector<E> {
    events: Arc<Mutex<E>>,
}

impl<E> TraceCollector<E>
where E: Default + Send + Sync {

    /// Get Events
    pub fn collect(&mut self) -> E {
        let mut events = self.events.lock();
        take(&mut events)
    }
    /// Run function on events ref
    pub fn collect_ref<F, O>(&self, f: F) -> O
    where F: FnOnce(&E) -> O {
        f(&*self.events.lock())
    }

    /// Run function on mut events ref
    pub fn collect_mut<F, O>(&mut self, f: F) -> O
    where F: FnOnce(&mut E) -> O {
        f(&mut *self.events.lock())
    }
}


#[derive(Debug, Error)]
pub enum TraceError {
    #[error("Trace Error")]
    DefaultError(),
    #[error("Collector Error: {0}")]
    TraceCollectorError(#[from] TraceCollectorError),
}


impl From<TraceError> for HookError<TraceError> {
    fn from(error: TraceError) -> HookError<TraceError> {
        HookError::Hook(error)
    }
}

impl From<TraceCollectorError> for HookError<TraceError> {
    fn from(error: TraceCollectorError) -> HookError<TraceError> {
        HookError::Hook(TraceError::TraceCollectorError(error))
    }
}



/// Trace observer
/// S: State
/// O: Order
/// E: Type for Event
pub struct TraceHook<S, O, E> {
    event_observer: Arc<dyn Fn(&Address, &PCode, &mut S, &mut E) -> Result<(), TraceCollectorError> + Send + Sync>,
    event_collector: Arc<Mutex<E>>,
    state: PhantomData<S>,
    order: PhantomData<O>,
}

// NOTE: manual implementation avoids adding the trait bound `E: Clone`.
impl<S, O, E> Clone for TraceHook<S, O, E> {
    fn clone(&self) -> Self {
        Self {
            event_observer: self.event_observer.clone(),
            event_collector: self.event_collector.clone(),
            state: PhantomData,
            order: PhantomData,
        }
    }
}

impl<S, O, E> TraceHook<S, O, E>
where
    S: AsState<PCodeState<u8, O>> + StateOps, 
    O: Order,
    E: Default + Send + Sync + 'static
{

    pub fn new_unboxed<F>(processor: F) -> (TraceCollector<E>, Self)
        where F: Fn(&Address, &PCode, &mut S, &mut E) -> Result<(), TraceCollectorError> + Send + Sync + 'static 
    {
        let event_collector = Arc::new(Mutex::new(Default::default()));
        let collector = TraceCollector {
            events: event_collector.clone(),
        };

        let observer = Self {
            event_observer: Arc::new(processor),
            event_collector,
            state: PhantomData,
            order: PhantomData,
        };

        (collector, observer)
    }
}
impl< S: 'static, O, E: 'static> HookConcrete for TraceHook<S, O, E>
where
    S: AsState<PCodeState<u8, O>> + StateOps, 
    O: Order
{
    type State = S;
    type Error = TraceError;
    type Outcome = String;
    
    
    fn hook_architectural_step(&mut self, state: &mut Self::State, address: &Address, step_state: &StepState)
        -> Result<HookOutcome<HookStepAction<Self::Outcome>>, HookError<Self::Error>> 
    {
        (*self.event_observer)(
            address,
            step_state.operations(),
            // instruction,
            state,
            &mut *self.event_collector.lock(),
        )?;
        Ok(HookStepAction::Pass.into())
    }
}
impl< S: 'static, O, E: 'static> ClonableHookConcrete for TraceHook<S, O, E>
where
    S: AsState<PCodeState<u8, O>> + StateOps, 
    O: Order
{
}



pub fn get_tbb_tace_obs<O: Order>()-> 
(TraceCollector<tbb::TBBBlocks>, 
	TraceHook<PCodeState<u8, O>, O, tbb::TBBBlocks>) {

	TraceHook::new_unboxed(
		|address,
		_pcode,
		// _instruction,
		_state: & mut PCodeState<u8, O>,
		collector: & mut tbb::TBBBlocks | -> Result<(), TraceCollectorError>{
			let mut tbb_block = tbb::TBBBlock::new();
			tbb_block.set_address(u64::from(address));
			tbb_block.set_n(1);
			collector.basic_blocks.push(tbb_block);
			Ok(())

	})
}
/// collect_tbb_trace_to_file()
/// collector: TraceCollector with TBBBlocks
/// file: trace in tbb format
/// file_plain: Trace in plain test, record pc changes, this can be read by flow_color.py in Ghidra
pub fn collect_tbb_trace_to_file(
	mut collector: TraceCollector<tbb::TBBBlocks>, 
	file : Option<&str>, file_plain : Option<&str>){

	let collect_res = collector.collect();
	// Write trace to file
	if file != None {
		// create trace file
		let path = Path::new(file.unwrap());
		let mut file = match File::create(&path){
			Err(why) => panic!("couldn't open {}: {}", path.display(), why),
			Ok(file) => file
		};
		// write trace
		let trace_b = collect_res.write_to_bytes().expect("parse trace to byte failed");
		file.write_all(&trace_b).expect("writting to trace file failed");
	}
	// Trace in plain test, record pc changes
	if file_plain != None {
			// create trace file
			let path = Path::new(file_plain.unwrap());
			let mut file = match File::create(&path){
				Err(why) => panic!("couldn't open {}: {}", path.display(), why),
				Ok(file) => file
			}; 
			for i in collect_res.get_basic_blocks() {
				let addr = i.get_address();
				file.write(format!("{:#x}\n", addr).as_bytes()).expect("unable to write to file");
			}

	}
	println!("{:?} PC changes has been logged", collect_res.basic_blocks.len());
}