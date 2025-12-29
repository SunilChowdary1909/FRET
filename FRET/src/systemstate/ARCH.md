# System-state heuristics
## Information flow
- ``fuzzer.rs`` resolves symbols and creates ``api_ranges`` and ``isr_ranges``
- ``helpers::QemuSystemStateHelper`` captures a series of ``RawFreeRTOSSystemState``
- ``observers::QemuSystemStateObserver`` divides this into ``ReducedFreeRTOSSystemState`` and ``ExecInterval``, the first contains the raw states and the second contains information about the flow between states
- ``stg::StgFeedback`` builds an stg from the intervals
## Target-specific (systemstate/target_os)
- config ``add_target_symbols`` and ``get_range_groups`` resolve important symbols
- provides a helper (e.g. ``FreeRTOSSystemStateHelper`` ) to capture the state
    - collects locally into e.g. ``CURRENT_SYSTEMSTATE_VEC``
    - post-processing
    - replaces ``SystemTraceData`` in state metadata