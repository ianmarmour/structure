# Implementation-dependent parameters (annex D)

This document specifies the values of all implementation-dependent parameters
listed in table D.1 of IEC 61131-3:2003, as required by subclause 1.5.1 b).
Use `structure check --report-implementation-dependent` to have uses of
implementation-dependent features reported (1.5.1 g).

| Subclause | Parameter | Value in this implementation |
|---|---|---|
| 2.1.2 | Maximum length of identifiers | Unlimited (all characters are significant). |
| 2.1.5 | Maximum comment length | Unlimited. |
| 2.1.6 | Syntax and semantics of pragmas | Any text between `{` and `}` is accepted as a pragma and ignored. Pragmas have no effect on the program. |
| 2.2.2 | Use of the double-quote character when feature 4 but not feature 2 of table 5 is supported | Not applicable: both features 2 and 4 are supported. |
| 2.3.1 | Range of values and precision of TIME, DATE, TIME_OF_DAY and DATE_AND_TIME | All four are 64-bit signed integers counting microseconds. TIME: -9223372036854775808 µs to 9223372036854775807 µs (about ±292 471 years). TIME_OF_DAY: 00:00:00 to 23:59:59.999999. DATE: D#0001-01-01 to D#9999-12-31 (proleptic Gregorian calendar). DATE_AND_TIME: DT#0001-01-01-00:00:00 to DT#9999-12-31-23:59:59.999999. |
| 2.3.1 | Precision of representation of seconds in TIME, TIME_OF_DAY and DATE_AND_TIME | 1 µs. Literals with finer resolution are rounded to the nearest microsecond. |
| 2.3.3.1 | Maximum number of enumerated values | 2 147 483 647 per type (values are stored as 32-bit integers). |
| 2.3.3.1 | Maximum number of array subscripts | Unlimited. |
| 2.3.3.1 | Maximum array size | 4 294 967 295 bytes (total memory of an image is limited by the host). |
| 2.3.3.1 | Maximum number of structure elements | Unlimited. |
| 2.3.3.1 | Maximum structure size | Limited by memory. |
| 2.3.3.1 | Maximum range of subscript values | -2 147 483 648 to 2 147 483 647. |
| 2.3.3.1 | Maximum number of levels of nested structures | Unlimited (64 for references resolved by the linker). |
| 2.3.3.2 | Default maximum length of STRING and WSTRING variables | 254 characters. |
| 2.3.3.2 | Maximum allowed length of STRING and WSTRING variables | 65 535 characters. |
| 2.4.1.1 | Maximum number of hierarchical levels | Unlimited. |
| 2.4.1.1 | Logical or physical mapping | Each resource has three process image areas: input (`I`), output (`Q`) and memory (`M`), each a byte array; multi-byte values are little-endian. `%IXn`/`%In`: bit *n* (byte *n*/8, bit *n* mod 8). `%IXa.b`: bit *b* (0 to 7) of byte *a*. `%IBn`, `%IWn`, `%IDn`, `%ILn`: element *n* of size 1, 2, 4 or 8 bytes, i.e. byte offset *n* × size (so `%IW6` is bytes 12 and 13, and `%IB12` is its low byte). With more levels, all fields but the last (for X: all but the last two) form a module path selecting a separate area of the same kind, e.g. `%IW2.5.7.1` is word 1 of module 2.5.7. Arrays located at an address occupy consecutive elements (BOOL arrays consecutive bits). Directly represented variables of the configuration-level VAR_GLOBAL belong to the first resource. The host accesses the areas with `Runtime::io(resource, 'I'|'Q'|'M')`. |
| 2.4.2 | Initialization of system inputs | Input areas are cleared (all zero) at every start; initial values declared for located input variables are written at start. Afterwards inputs are only changed by the host. |
| 2.4.3 | Maximum number of variables per declaration | Unlimited. |
| 2.4.3 | Effect of using the AT qualifier in the declaration of function block instances | Not allowed (error). |
| 2.4.3 | Warm start behavior of variables declared neither RETAIN nor NON_RETAIN | They take their initial values (they are not retentive). |
| 2.5 | Information to determine execution times of POUs | The virtual machine executes one bytecode instruction at a time; with the simulated clock every instruction takes `--ns-per-instruction` nanoseconds (default 20). `Runtime::job_stats` reports the number of instructions of the longest execution of every program and task-associated function block. |
| 2.5.1.2 | Values of outputs when ENO is FALSE | Function result: a value with all bits zero (0, FALSE, 0.0, T#0s, the empty string, the first enumerated value). VAR_OUTPUT and VAR_IN_OUT: unchanged. In LD and FBD, variables connected to the outputs of a function whose ENO is FALSE are not assigned. |
| 2.5.1.3 | Maximum number of function specifications | Unlimited. |
| 2.5.1.5 | Maximum number of inputs of extensible functions | 255. |
| 2.5.1.5.1 | Effects of type conversions on accuracy | Conversions are value preserving. Integer to integer and to bit string (and back): exact; a value that is not representable in the destination type is a type conversion error. REAL/LREAL to integer: rounding to the nearest integer, ties to even (table 22 footnote b); TRUNC truncates toward zero. Integer to REAL/LREAL: rounding to the nearest representable value. LREAL to REAL: rounding to nearest. Any type to BOOL: zero is FALSE, any other value TRUE. BOOL to numbers: 0 or 1. Conversions to STRING/WSTRING produce the external representation of 2.2 (`TRUE`, `-42`, `1.5`, `1.0E+20`, `T#1h30m`, `D#1984-06-25`, `TOD#15:36:55.36`, `DT#1984-06-25-15:36:55.36`; bit strings in decimal). Conversions from strings accept the same representations (with or without type prefix, based integers allowed). |
| 2.5.1.5.1 | Error conditions during type conversions | Value out of range of the destination type; NaN or infinity converted to an integer; a string not conforming to the external representation; a WSTRING character above 16#FF converted to STRING; invalid BCD digits; a negative value or overflow in conversions to BCD. All raise the run-time error "type conversion error" (or set ENO to FALSE when the function is invoked with EN/ENO). |
| 2.5.1.5.2 | Accuracy of numerical functions | REAL functions are computed in double precision and rounded to single precision; LREAL functions use the host's IEEE 754 double precision library (typically within 1 ulp). A result that is not finite for finite inputs (overflow, SQRT/LN/LOG of a negative number, LN(0), ASIN/ACOS outside [-1, 1], 0.0 ** negative) raises "numerical result exceeds range for data type". |
| 2.5.1.5.6 | Effects of type conversions between time data types and other data types not defined in table 30 | TIME and TIME_OF_DAY convert to and from numbers in milliseconds (integer conversions truncate toward zero; REAL/LREAL keep fractions of milliseconds). DATE and DATE_AND_TIME convert to and from numbers as seconds since 1970-01-01T00:00:00. DATE_TO_DT, TOD_TO_TIME, TIME_TO_TOD (range checked) are also provided. |
| 2.5.2 | Maximum number of function block specifications and instantiations | Unlimited. |
| 2.5.2.1a | Function block input variable assignment when EN is FALSE | Input values are assigned; only the body is not executed (ENO := FALSE, outputs keep their values). |
| 2.5.2.3.3 | PVmin, PVmax of counters | The smallest and largest values of the type of CV (e.g. -32768 and 32767 for CTU, CTD, CTUD; 0 for the unsigned variants' minimum). |
| 2.5.2.3.4 | Effect of a change in the value of a PT input during a timing operation | The new value is used from the next execution on: the timer compares the elapsed time with the current PT (so lowering PT can end an interval immediately; for TP, setting PT to T#0s ends the pulse at the next execution). |
| 2.5.3 | Program size limitations | None other than host memory. |
| 2.6.2 | Precision of step elapsed time | 1 µs (time of the task clock at the start of each execution of the POU). |
| 2.6.2 | Maximum number of steps per SFC | Unlimited (the static analysis of unsafe and unreachable SFCs is performed for networks of up to 128 steps and 20 000 reachable states). |
| 2.6.3 | Maximum number of transitions per SFC and per step | Unlimited. |
| 2.6.4.2 | Maximum number of action blocks per step | Unlimited. |
| 2.6.4.5 | Access to the functional equivalent of the Q or A outputs | `action.Q` and `action.A` can be read (read-only) anywhere in the POU containing the action, and through the run-time variable interface. Table 45a feature 1 (with "final scan") is implemented. |
| 2.6.4.1 | Execution of SFC elements included in an action (table 42 feature 2s) | The included chart is enclosed by the action, as with the enclosing steps of IEC 60848: when the action becomes active (Q rising) the initial step of the chart is activated; each execution of the action while it is active performs one scan of the chart (step times, action control, actions, evolution); in the final scan of the action all steps of the chart are deactivated, which is also the final scan of its active actions. While the action is inactive the chart is not executed and none of its steps is active. Steps and actions of included charts are local to the POU, like all others; an action can only be associated with steps of one chart. |
| 2.6.5 | Transition clearing time | Transitions are evaluated and cleared once per execution of the POU, after the actions of that execution; activation and deactivation take effect immediately. |
| 2.6.5 | Maximum width of diverge/converge constructs | Unlimited. |
| 2.7.1 | Contents of RESOURCE libraries | Any identifier is accepted as resource type name after ON; all resources execute on the virtual machine. |
| 2.7.1 | Effect of using READ_WRITE access to function block outputs | Allowed; the written value is overwritten by the next execution of the function block. |
| 2.7.2 | Maximum number of tasks | Unlimited. |
| 2.7.2 | Task interval resolution | 1 µs. |
| 3.3.1 | Maximum length of expressions | Unlimited. |
| 3.3.2 | Maximum length of statements | Unlimited. |
| 3.3.2.3 | Maximum number of CASE selections | Unlimited. |
| 3.3.2.4 | Value of control variable upon termination of FOR loop | The last value for which the statements were executed (or the initial value if they were never executed); the control variable is not incremented past the final value. |
| 4.1.1 | Restrictions on network topology | None. Networks are delimited by connectivity; connectors join networks. |
| 4.1.3 | Evaluation order of feedback loops | Elements are evaluated as soon as all their inputs are known, top to bottom then left to right among elements that are ready. An explicit loop is broken at a net connected to a variable (the feedback variable): its value before the evaluation of the network is used. A loop without such a variable is an error. |
| 4.3.3 | Order of evaluation of FBD networks (means for the user to determine it) | Networks are evaluated top to bottom in the order in which they are drawn, except as modified by jumps and returns. |

## Additional parameters

| Item | Value |
|---|---|
| Scheduling | Preemptive (table 50 feature 5b) by default; non-preemptive (5a) with `--non-preemptive` / `Options::preemptive = false`. Preemption happens between bytecode instructions. A program with no task association runs continuously at the lowest priority. |
| Task deadline | A task activation while an earlier activation of one of its POUs has not completed is a missed deadline (error 2.7.2); the activation is skipped. |
| Watchdog | An execution of a POU exceeding 50 000 000 instructions raises "iteration fails to terminate" (`Options::watchdog`). |
| Data stack | Computed from the call graph at link time for each program and task-associated function block. |
| Task-associated function blocks | Executed on a private copy of the instance; inputs are read from, and outputs and internal state written back to, the instance atomically (2.7.2 rule 7). Invocations in the body of the containing program assign the inputs but do not execute the function block (rule 5). |
| Error handling | Run-time errors stop the resource in which they occur (default) or abandon the faulting execution and continue (`--on-error continue`). |
