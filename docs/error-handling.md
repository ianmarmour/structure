# Treatment of error conditions (annex E)

Subclause 1.5.1 d) requires every error condition of table E.1 to be
treated in at least one of four ways. This implementation uses:

* **(3)** the error is reported during preparation of the program for
  execution (`structure check`, `structure build`, `structure run`), and
  no executable image is produced;
* **(2)** the system reports during preparation that an occurrence of the
  error is possible;
* **(4)** the error is reported during execution and error handling is
  initiated: by default the resource in which the error occurs is stopped
  (its tasks are disabled); with `--on-error continue`
  (`ErrorPolicy::AbortExecution`) the faulting execution of the program
  organization unit is abandoned and scheduling continues. Errors raised by
  standard functions invoked with EN/ENO reset ENO to FALSE instead
  (2.5.1.2 rule 3) and execution continues.

No error is treated by method (1) (not reported). Run-time errors are
reported with the resource, the program or function block instance, the
routine and the source location (file, line, column).

| Subclause | Error condition | Treatment |
|---|---|---|
| 2.1.5 | Nested comments | (3) |
| 2.3.3.1 | Ambiguous enumerated value | (3) |
| 2.3.3.1 | Value of a variable exceeds the specified subrange | (3) for constant values (initial values, literals assigned to subrange variables, case labels); (4) for all other assignments (each store into a subrange variable is checked) |
| 2.4.1.1 | Missing configuration of an incomplete address specification ("*" notation) | (3) at link time, for every instance |
| 2.4.3 | Attempt by a POU to modify a variable declared CONSTANT | (3); attempts through a READ_WRITE access path are prevented (access to CONSTANT variables must be READ_ONLY, otherwise (3)) |
| 2.4.3 | VAR_GLOBAL CONSTANT in a containing element and VAR_EXTERNAL without CONSTANT in a contained element | (3) at link time |
| 2.5.1 | Improper use of directly represented or external variables in functions | (3) (VAR_EXTERNAL, located variables and directly represented variables are rejected in functions; so are function block instances, edge-sensing contacts/coils and RETAIN/NON_RETAIN, which would give functions internal state) |
| 2.5.1.1 | A VAR_IN_OUT variable is not "properly mapped" | (3) (the actual argument must be a non-constant variable of the invoking POU — not an input, not an output of another function block, not an expression); in IL, an in-out left unassigned by `CAL` is detected by (4) when the reference is used |
| 2.5.1.1 | Ambiguous value caused by a VAR_IN_OUT connection | (3): in textual languages an in-out variable is always connected to exactly one variable; in FBD/LD the right-hand connection of an in-out variable may only lead to variables (figure 12 d) and is reported if it reaches more than one variable |
| 2.5.1.5.1 | Type conversion errors | (4) (see annex D for the conditions); (3) for conversions of constants that are out of range |
| 2.5.1.5.2 | Numerical result exceeds range for data type | (4); (3) for constant expressions |
| 2.5.1.5.2 | Division by zero | (4); (3) for constant expressions |
| 2.5.1.5.3 | N input is less than zero in a bit-shift function | (4) |
| 2.5.1.5.4 | Mixed input data types to a selection function | (3) |
| 2.5.1.5.4 | Selector (K) out of range for MUX function | (4) |
| 2.5.1.5.5 | Invalid character position specified | (4) |
| 2.5.1.5.5 | Result exceeds maximum string length | (4) (65 535 characters; assignments to variables of a smaller declared length truncate) |
| 2.5.1.5.5 | ANY_INT input is less than zero in a string function | (4) |
| 2.5.1.5.6 | Result exceeds range for data type (time data types) | (4) |
| 2.5.2.2 | No value specified for a function block instance used as input variable | (3) for functions; (4) for function blocks (the reference is checked when used, and invoking a null instance is an error) |
| 2.5.2.2 | No value specified for an in-out variable | (3) for every invocation in ST, LD and FBD; (4) in IL when the in-out was never assigned |
| 2.6.2 | Zero or more than one initial steps in SFC network | (3) |
| 2.6.2 | User program attempts to modify step state or time | (3) |
| 2.6.3 | Side effects in evaluation of transition condition | (3): assignments, function block invocations, functions with outputs connected, RETURN and jumps are rejected in transition conditions |
| 2.6.4.5 | Action control contention error | (4) (more than one active timed association, SD while SL active, SL while SD active) |
| 2.6.5 | Simultaneously true, non-prioritized transitions in a selection divergence | (4) for graphical selection divergences drawn without priority (table 46 feature 2c). Textual transitions are prioritized in the order of their declaration (or by PRIORITY), so no conflict can arise |
| 2.6.5 | Unsafe or unreachable SFC | (2): a static analysis of the reachable states of each SFC (ignoring transition conditions) reports transitions that can activate an already active step (unsafe), transitions that can never be enabled and steps that can never become active (unreachable); (4): activating an already active step at run time is reported |
| 2.7.1 | Data type conflict in VAR_ACCESS | (3) |
| 2.7.2 | A task fails to be scheduled or to meet its execution deadline | (4): reported for each activation that is skipped because the previous activation has not completed; the run-time statistics (`task_overruns`) count them |
| 3.2.2 | Numerical result exceeds range for data type (IL) | (4) |
| 3.2.2 | Current result and operand not of same data type | (3) |
| 3.3.1 | Division by zero | (4); (3) for constant expressions |
| 3.3.1 | Numerical result exceeds range for data type | (4); (3) for constant expressions |
| 3.3.1 | Invalid data type for operation | (3) |
| 3.3.2.1 | Return from function without value assigned | (4) (a check is compiled into every function whose body does not assign the result before any RETURN) |
| 3.3.2.4 | Iteration fails to terminate | (4): an execution of a POU exceeding the watchdog limit (50 000 000 instructions by default) is stopped |
| 4.1.1 | Same identifier used as connector label and element name | (3) |
| 4.1.3 | Uninitialized feedback variable | (3): an explicit feedback loop must be connected to a variable (whose initial value is then used) |

## Other violations (1.5.1 c)

Violations of requirements of the standard that are not errors of
annex E (syntax errors, undeclared names, misuse of keywords and
reserved names, recursion, assignment to inputs or to outputs of other
function blocks, reading function block inputs or internal variables from
outside, generic types in user declarations, invalid qualifiers, located
variables in the wrong place, duplicate declarations, etc.) are all
reported during preparation. Uses of extensions (see `structure
extensions`) are reported as warnings, or as errors with `--strict`.
