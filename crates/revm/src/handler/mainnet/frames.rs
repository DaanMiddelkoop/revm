use crate::{
    db::Database,
    interpreter::{
        CallContext, CallInputs, CreateInputs, InterpreterResult, SharedMemory, Transfer,
    },
    primitives::{Spec, TransactTo},
    CallStackFrame, Context, FrameOrResult,
};
use alloc::boxed::Box;
use core::ops::Range;
use revm_interpreter::CallScheme;

/// Creates first fmrae
pub fn create_first_frame<SPEC: Spec, EXT, DB: Database>(
    context: &mut Context<EXT, DB>,
    gas_limit: u64,
) -> FrameOrResult {
    // call inner handling of call/create
    match context.evm.env.tx.transact_to {
        TransactTo::Call(address) => context.evm.make_call_frame(
            &CallInputs {
                contract: address,
                transfer: Transfer {
                    source: context.evm.env.tx.caller,
                    target: address,
                    value: context.evm.env.tx.value,
                },
                input: context.evm.env.tx.data.clone(),
                gas_limit,
                context: CallContext {
                    caller: context.evm.env.tx.caller,
                    address,
                    code_address: address,
                    apparent_value: context.evm.env.tx.value,
                    scheme: CallScheme::Call,
                },
                is_static: false,
            },
            0..0,
        ),
        TransactTo::Create(scheme) => context.evm.make_create_frame(
            SPEC::SPEC_ID,
            &CreateInputs {
                caller: context.evm.env.tx.caller,
                scheme,
                value: context.evm.env.tx.value,
                init_code: context.evm.env.tx.data.clone(),
                gas_limit,
            },
        ),
    }
}

/// Handle frame return.
pub fn handle_frame_return<SPEC: Spec, EXT, DB: Database>(
    context: &mut Context<EXT, DB>,
    child_stack_frame: Box<CallStackFrame>,
    parent_stack_frame: Option<&mut Box<CallStackFrame>>,
    shared_memory: &mut SharedMemory,
    result: InterpreterResult,
) -> Option<InterpreterResult> {
    // break from loop if this is last CallStackFrame.
    if child_stack_frame.is_create {
        let Some(parent_stack_frame) = parent_stack_frame else {
            return Some(
                context
                    .evm
                    .create_return::<SPEC>(result, child_stack_frame)
                    .0,
            );
        };
        let (result, address) = context.evm.create_return::<SPEC>(result, child_stack_frame);
        parent_stack_frame
            .interpreter
            .insert_create_output(result, Some(address))
    } else {
        let Some(parent_stack_frame) = parent_stack_frame else {
            return Some(context.evm.call_return(result, child_stack_frame));
        };
        let subcall_memory_return_offset = child_stack_frame.subcall_return_memory_range.clone();
        let result = context.evm.call_return(result, child_stack_frame);

        parent_stack_frame.interpreter.insert_call_output(
            shared_memory,
            result,
            subcall_memory_return_offset,
        )
    }
    None
}

/// Handle frame sub call.
pub fn handle_frame_sub_call<SPEC: Spec, EXT, DB: Database>(
    context: &mut Context<EXT, DB>,
    inputs: Box<CallInputs>,
    curent_stack_frame: &mut CallStackFrame,
    shared_memory: &mut SharedMemory,
    return_memory_offset: Range<usize>,
) -> Option<Box<CallStackFrame>> {
    match context
        .evm
        .make_call_frame(&inputs, return_memory_offset.clone())
    {
        FrameOrResult::Frame(new_frame) => Some(new_frame),
        FrameOrResult::Result(result) => {
            curent_stack_frame.interpreter.insert_call_output(
                shared_memory,
                result,
                return_memory_offset,
            );
            None
        }
    }
}

/// Handle frame sub create.
pub fn handle_frame_sub_create<SPEC: Spec, EXT, DB: Database>(
    context: &mut Context<EXT, DB>,
    curent_stack_frame: &mut CallStackFrame,
    inputs: Box<CreateInputs>,
) -> Option<Box<CallStackFrame>> {
    match context.evm.make_create_frame(SPEC::SPEC_ID, &inputs) {
        FrameOrResult::Frame(new_frame) => Some(new_frame),
        FrameOrResult::Result(result) => {
            // insert result of the failed creation of create CallStackFrame.
            curent_stack_frame
                .interpreter
                .insert_create_output(result, None);
            None
        }
    }
}
