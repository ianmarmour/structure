# Compliance statement (IEC 61131-3:2003, subclause 1.5.1)

This system complies with the requirements of IEC 61131-3, for the following
language features:

## Common elements

| Table No. | Feature No. | Features description |
|---|---|---|
| 1 | 2 | Lower case characters |
| 1 | 3a | Number sign (#) |
| 1 | 3b | Pound sign (£) |
| 1 | 4a | Dollar sign ($) |
| 1 | 4b | Currency sign (¤) |
| 1 | 5a | Vertical bar (\|) |
| 2 | 1 | Upper case and numbers |
| 2 | 2 | Upper and lower case, numbers, embedded underlines |
| 2 | 3 | Upper and lower case, numbers, leading or embedded underlines |
| 3 | 1 | Comments |
| 3a | 1 | Pragmas |
| 4 | 1 | Integer literals |
| 4 | 2 | Real literals |
| 4 | 3 | Real literals with exponents |
| 4 | 4 | Base 2 literals |
| 4 | 5 | Base 8 literals |
| 4 | 6 | Base 16 literals |
| 4 | 7 | Boolean zero and one |
| 4 | 8 | Boolean FALSE and TRUE |
| 4 | 9 | Typed literals |
| 5 | 1 | Single-byte character strings |
| 5 | 2 | Double-byte character strings |
| 5 | 3 | Single-byte typed string literals |
| 5 | 4 | Double-byte typed string literals |
| 6 | 2 | $$ Dollar sign |
| 6 | 3 | $' Single quote |
| 6 | 4 | $L or $l Line feed |
| 6 | 5 | $N or $n Newline |
| 6 | 6 | $P or $p Form feed (page) |
| 6 | 7 | $R or $r Carriage return |
| 6 | 8 | $T or $t Tab |
| 6 | 9 | $" Double quote |
| 7 | 1a | Duration literals without underlines: short prefix |
| 7 | 1b | Duration literals without underlines: long prefix |
| 7 | 2a | Duration literals with underlines: short prefix |
| 7 | 2b | Duration literals with underlines: long prefix |
| 8 | 1 | Date literals (long prefix) |
| 8 | 2 | Date literals (short prefix) |
| 8 | 3 | Time of day literals (long prefix) |
| 8 | 4 | Time of day literals (short prefix) |
| 8 | 5 | Date and time literals (long prefix) |
| 8 | 6 | Date and time literals (short prefix) |
| 10 | 1 | BOOL |
| 10 | 2 | SINT |
| 10 | 3 | INT |
| 10 | 4 | DINT |
| 10 | 5 | LINT |
| 10 | 6 | USINT |
| 10 | 7 | UINT |
| 10 | 8 | UDINT |
| 10 | 9 | ULINT |
| 10 | 10 | REAL |
| 10 | 11 | LREAL |
| 10 | 12 | TIME |
| 10 | 13 | DATE |
| 10 | 14 | TIME_OF_DAY or TOD |
| 10 | 15 | DATE_AND_TIME or DT |
| 10 | 16 | STRING |
| 10 | 17 | BYTE |
| 10 | 18 | WORD |
| 10 | 19 | DWORD |
| 10 | 20 | LWORD |
| 10 | 21 | WSTRING |
| 12 | 1 | Direct derivation from elementary types |
| 12 | 2 | Enumerated data types |
| 12 | 3 | Subrange data types |
| 12 | 4 | Array data types |
| 12 | 5 | Structured data types |
| 14 | 1 | Initialization of directly derived types |
| 14 | 2 | Initialization of enumerated data types |
| 14 | 3 | Initialization of subrange data types |
| 14 | 4 | Initialization of array data types |
| 14 | 5 | Initialization of structured data type elements |
| 14 | 6 | Initialization of derived structured data types |
| 15 | 1 | I: Input location |
| 15 | 2 | Q: Output location |
| 15 | 3 | M: Memory location |
| 15 | 4 | X: Single bit size |
| 15 | 5 | None: Single bit size |
| 15 | 6 | B: Byte (8 bits) size |
| 15 | 7 | W: Word (16 bits) size |
| 15 | 8 | D: Double word (32 bits) size |
| 15 | 9 | L: Long (quad) word (64 bits) size |
| 15 | 10 | Use of an asterisk (*) to indicate a not yet specified location |
| 17 | 1 | Declaration of directly represented variables |
| 17 | 2 | Declaration of directly represented retentive variables |
| 17 | 3 | Declaration of locations of symbolic variables |
| 17 | 4 | Array location assignment |
| 17 | 5 | Automatic memory allocation of symbolic variables |
| 17 | 6 | Array declaration |
| 17 | 7 | Retentive array declaration |
| 17 | 8 | Declaration of structured variables |
| 18 | 1 | Initialization of directly represented variables |
| 18 | 2 | Initialization of directly represented retentive variables |
| 18 | 3 | Location and initial value assignment to symbolic variables |
| 18 | 4 | Array location assignment and initialization |
| 18 | 5 | Initialization of symbolic variables |
| 18 | 6 | Array initialization |
| 18 | 7 | Retentive array declaration and initialization |
| 18 | 8 | Initialization of structured variables |
| 18 | 9 | Initialization of constants |
| 18 | 10 | Initialization of function block instances |
| 19 | 1 | Negated input |
| 19 | 2 | Negated output |
| 19a | 1 | Formal invocation of functions |
| 19a | 2 | Non-formal invocation of functions |
| 20 | 1 | Use of EN and ENO shown in LD |
| 20 | 2 | Usage without EN and ENO (FBD) |
| 20 | 3 | Usage with EN and without ENO (FBD) |
| 20 | 4 | Usage without EN and with ENO (FBD) |
| 20a | 1 | In-out variable declaration (textual) |
| 20a | 2 | In-out variable declaration (graphical) |
| 20a | 3 | Graphical connection of in-out variable to different variables |
| 21 | 1 | Overloaded functions |
| 21 | 2 | Typed functions |
| 22 | 1 | Type conversion functions `*_TO_**` |
| 22 | 2 | TRUNC |
| 22 | 3 | `*_BCD_TO_**` |
| 22 | 4 | `**_TO_BCD_*` |
| 23 | 1 | ABS |
| 23 | 2 | SQRT |
| 23 | 3 | LN |
| 23 | 4 | LOG |
| 23 | 5 | EXP |
| 23 | 6 | SIN |
| 23 | 7 | COS |
| 23 | 8 | TAN |
| 23 | 9 | ASIN |
| 23 | 10 | ACOS |
| 23 | 11 | ATAN |
| 24 | 12n, 12s | ADD, + |
| 24 | 13n, 13s | MUL, * |
| 24 | 14n, 14s | SUB, - |
| 24 | 15n, 15s | DIV, / |
| 24 | 16n | MOD |
| 24 | 17n, 17s | EXPT, ** |
| 24 | 18n | MOVE |
| 25 | 1 | SHL |
| 25 | 2 | SHR |
| 25 | 3 | ROR |
| 25 | 4 | ROL |
| 26 | 5n, 5s | AND, & |
| 26 | 6n | OR |
| 26 | 7n | XOR |
| 26 | 8 | NOT |
| 27 | 1 | SEL |
| 27 | 2a | MAX |
| 27 | 2b | MIN |
| 27 | 3 | LIMIT |
| 27 | 4 | MUX |
| 28 | 5n, 5s | GT, > |
| 28 | 6n, 6s | GE, >= |
| 28 | 7n, 7s | EQ, = |
| 28 | 8n, 8s | LE, <= |
| 28 | 9n, 9s | LT, < |
| 28 | 10n, 10s | NE, <> |
| 29 | 1 | LEN |
| 29 | 2 | LEFT |
| 29 | 3 | RIGHT |
| 29 | 4 | MID |
| 29 | 5 | CONCAT |
| 29 | 6 | INSERT |
| 29 | 7 | DELETE |
| 29 | 8 | REPLACE |
| 29 | 9 | FIND |
| 30 | 1an, 1as | ADD, + (TIME, TIME) |
| 30 | 1bn, 1bs | ADD_TIME |
| 30 | 2an, 2as | ADD, + (TIME_OF_DAY, TIME) |
| 30 | 2bn | ADD_TOD_TIME |
| 30 | 3an, 3as | ADD, + (DATE_AND_TIME, TIME) |
| 30 | 3bn | ADD_DT_TIME |
| 30 | 4an, 4as | SUB, - (TIME, TIME) |
| 30 | 4bn | SUB_TIME |
| 30 | 5an, 5as | SUB, - (DATE, DATE) |
| 30 | 5bn | SUB_DATE_DATE |
| 30 | 6an, 6as | SUB, - (TIME_OF_DAY, TIME) |
| 30 | 6bn | SUB_TOD_TIME |
| 30 | 7an, 7as | SUB, - (TIME_OF_DAY, TIME_OF_DAY) |
| 30 | 7bn | SUB_TOD_TOD |
| 30 | 8an, 8as | SUB, - (DATE_AND_TIME, TIME) |
| 30 | 8bn | SUB_DT_TIME |
| 30 | 9an, 9as | SUB, - (DATE_AND_TIME, DATE_AND_TIME) |
| 30 | 9bn | SUB_DT_DT |
| 30 | 10an, 10as | MUL, * (TIME, ANY_NUM) |
| 30 | 10bn | MULTIME |
| 30 | 11an, 11as | DIV, / (TIME, ANY_NUM) |
| 30 | 11bn | DIVTIME |
| 30 | 12 | CONCAT_DATE_TOD |
| 30 | 13 | DT_TO_TOD |
| 30 | 14 | DT_TO_DATE |
| 31 | 1 | SEL |
| 31 | 2 | MUX |
| 31 | 3n, 3s | EQ, = |
| 31 | 4n, 4s | NE, <> |
| 33 | 1a | RETAIN qualifier on internal variables |
| 33 | 1b | NON_RETAIN qualifier on internal variables |
| 33 | 2a | RETAIN qualifier on output variables |
| 33 | 2b | RETAIN qualifier on input variables |
| 33 | 2c | NON_RETAIN qualifier on output variables |
| 33 | 2d | NON_RETAIN qualifier on input variables |
| 33 | 3a | RETAIN qualifier on internal function blocks |
| 33 | 3b | NON_RETAIN qualifier on internal function blocks |
| 33 | 4a | VAR_IN_OUT declaration (textual) |
| 33 | 4b | VAR_IN_OUT declaration and usage (graphical) |
| 33 | 4c | VAR_IN_OUT declaration with assignment to different variables (graphical) |
| 33 | 5a | Function block instance name as input (textual) |
| 33 | 5b | Function block instance name as input (graphical) |
| 33 | 6a | Function block instance name as VAR_IN_OUT (textual) |
| 33 | 6b | Function block instance name as VAR_IN_OUT (graphical) |
| 33 | 7a | Function block instance name as external variable (textual) |
| 33 | 7b | Function block instance name as external variable (graphical) |
| 33 | 8a | Textual declaration of rising edge inputs |
| 33 | 8b | Textual declaration of falling edge inputs |
| 33 | 9a | Graphical declaration of rising edge inputs |
| 33 | 9b | Graphical declaration of falling edge inputs |
| 33 | 10a | VAR_EXTERNAL declarations within function block type declarations |
| 33 | 10b | VAR_EXTERNAL CONSTANT declarations within function block type declarations |
| 33 | 11 | VAR_TEMP declarations within function block type declarations |
| 34 | 1 | Bistable function block (set dominant) SR |
| 34 | 2 | Bistable function block (reset dominant) RS |
| 35 | 1 | Rising edge detector R_TRIG |
| 35 | 2 | Falling edge detector F_TRIG |
| 36 | 1a | CTU |
| 36 | 1b | CTU_DINT |
| 36 | 1c | CTU_LINT |
| 36 | 1d | CTU_UDINT |
| 36 | 1e | CTU_ULINT |
| 36 | 2a | CTD |
| 36 | 2b | CTD_DINT |
| 36 | 2c | CTD_LINT |
| 36 | 2d | CTD_UDINT |
| 36 | 2e | CTD_ULINT |
| 36 | 3a | CTUD |
| 36 | 3b | CTUD_DINT |
| 36 | 3c | CTUD_LINT |
| 36 | 3d | CTUD_ULINT |
| 37 | 1 | TP (pulse) |
| 37 | 2a | TON (on-delay) |
| 37 | 3a | TOF (off-delay) |
| 39 | 1 to 8b | Same as features 1 to 8b of table 33 |
| 39 | 10 | Formal input and output variables |
| 39 | 11 to 14 | Same as features 1 to 4 of table 17 |
| 39 | 15 to 18 | Same as features 1 to 4 of table 18 |
| 39 | 19 | Use of directly represented variables |
| 39 | 20 | VAR_GLOBAL...END_VAR declaration within a PROGRAM |
| 39 | 21 | VAR_ACCESS...END_VAR declaration within a PROGRAM |
| 39 | 22a | VAR_EXTERNAL declarations within PROGRAM type declarations |
| 39 | 22b | VAR_EXTERNAL CONSTANT declarations within PROGRAM type declarations |
| 39 | 23 | VAR_GLOBAL CONSTANT declarations within PROGRAM type declarations |
| 39 | 24 | VAR_TEMP declarations within PROGRAM type declarations |
| 40 | 1 | Step and initial step - graphical form |
| 40 | 2 | Step and initial step - textual form |
| 40 | 3a | Step flag - general form |
| 40 | 4 | Step elapsed time - general form |
| 41 | 1 | Transition condition adjacent to the transition using ST language |
| 41 | 2 | Transition condition adjacent to the transition using LD language |
| 41 | 3 | Transition condition adjacent to the transition using FBD language |
| 41 | 4 | Use of a transition connector |
| 41 | 4a | Transition connector condition using LD language |
| 41 | 4b | Transition connector condition using FBD language |
| 41 | 5 | Textual transition using ST language |
| 41 | 6 | Textual transition using IL language |
| 41 | 7 | Use of a transition name |
| 41 | 7a | Transition name, condition using LD language |
| 41 | 7b | Transition name, condition using FBD language |
| 41 | 7c | Transition name, condition using IL language |
| 41 | 7d | Transition name, condition using ST language |
| 42 | 1 | Boolean variable as action |
| 42 | 2l | Graphical declaration of actions in LD language |
| 42 | 2f | Graphical declaration of actions in FBD language |
| 42 | 2s | Inclusion of SFC elements in actions |
| 42 | 3s | Textual declaration of actions in ST language |
| 42 | 3i | Textual declaration of actions in IL language |
| 43 | 1 | Action block physically or logically adjacent to the step |
| 43 | 2 | Concatenated action blocks |
| 43 | 3 | Textual step body |
| 43 | 4 | Action block "d" field |
| 44 | 1 | "a": Qualifier |
| 44 | 2 | "b": Action name |
| 44 | 3 | "c": Boolean "indicator" variables |
| 44 | 4 | "d": Action using IL language |
| 44 | 5 | "d": Action using ST language |
| 44 | 6 | "d": Action using LD language |
| 44 | 7 | "d": Action using FBD language |
| 44 | 8 | Use of action blocks in ladder diagrams |
| 44 | 9 | Use of action blocks in function block diagrams |
| 45 | 1 | None (non-stored) |
| 45 | 2 | N: Non-stored |
| 45 | 3 | R: overriding Reset |
| 45 | 4 | S: Set (Stored) |
| 45 | 5 | L: time Limited |
| 45 | 6 | D: time Delayed |
| 45 | 7 | P: Pulse |
| 45 | 8 | SD: Stored and time Delayed |
| 45 | 9 | DS: Delayed and Stored |
| 45 | 10 | SL: Stored and time Limited |
| 45 | 11 | P1: Pulse (rising edge) |
| 45 | 12 | P0: Pulse (falling edge) |
| 45a | 1 | Action control per figures 14 a) and 15 a) (with "final scan") |
| 46 | 1 | Single sequence |
| 46 | 2a | Divergence of sequence selection, left-to-right priority (textually: declaration order) |
| 46 | 2b | Divergence of sequence selection, numbered branches (textually: PRIORITY) |
| 46 | 2c | Divergence of sequence selection without priority (graphical) |
| 46 | 3 | Convergence of sequence selection |
| 46 | 4 | Simultaneous sequences - divergence and convergence |
| 46 | 5a, 5b, 5c | Sequence skip |
| 46 | 6a, 6b, 6c | Sequence loop |
| 46 | 7 | Directional arrows |
| 49 | 1 | CONFIGURATION...END_CONFIGURATION construction |
| 49 | 2 | VAR_GLOBAL...END_VAR construction within CONFIGURATION |
| 49 | 3 | RESOURCE...ON...END_RESOURCE construction |
| 49 | 4 | VAR_GLOBAL...END_VAR construction within RESOURCE |
| 49 | 5a | Periodic TASK construction |
| 49 | 5b | Non-periodic TASK construction |
| 49 | 6a | WITH construction for PROGRAM to TASK association |
| 49 | 6b | WITH construction for Function Block to TASK association |
| 49 | 6c | PROGRAM declaration with no TASK association |
| 49 | 7 | Declaration of directly represented variables in VAR_GLOBAL |
| 49 | 8a | Connection of directly represented variables to PROGRAM inputs |
| 49 | 8b | Connection of GLOBAL variables to PROGRAM inputs |
| 49 | 9a | Connection of PROGRAM outputs to directly represented variables |
| 49 | 9b | Connection of PROGRAM outputs to GLOBAL variables |
| 49 | 10a | VAR_ACCESS...END_VAR construction |
| 49 | 10b | Access paths to directly represented variables |
| 49 | 10c | Access paths to PROGRAM inputs |
| 49 | 10d | Access paths to GLOBAL variables in RESOURCEs |
| 49 | 10e | Access paths to GLOBAL variables in CONFIGURATIONs |
| 49 | 10f | Access paths to PROGRAM outputs |
| 49 | 10g | Access paths to PROGRAM internal variables |
| 49 | 10h | Access paths to function block inputs |
| 49 | 10i | Access paths to function block outputs |
| 49 | 11 | VAR_CONFIG...END_VAR construction |
| 49 | 12a | VAR_GLOBAL CONSTANT in RESOURCE declarations |
| 49 | 12b | VAR_GLOBAL CONSTANT in CONFIGURATION declarations |
| 49 | 13a | VAR_EXTERNAL in RESOURCE declarations |
| 49 | 13b | VAR_EXTERNAL CONSTANT in RESOURCE declarations |
| 50 | 1a | Textual declaration of periodic TASK |
| 50 | 1b | Textual declaration of non-periodic TASK |
| 50 | 2a | Graphical representation of periodic TASKs |
| 50 | 2b | Graphical representation of non-periodic TASKs |
| 50 | 3a | Textual association with PROGRAMs |
| 50 | 3b | Textual association with function blocks |
| 50 | 4a | Graphical association with PROGRAMs |
| 50 | 4b | Graphical association with function blocks within PROGRAMs |
| 50 | 5a | Non-preemptive scheduling (option `--non-preemptive`) |
| 50 | 5b | Preemptive scheduling (default) |

The overloaded and typed forms of the standard functions (table 21,
footnote a): every standard function defined as overloaded in 2.5.1.5 is
overloaded on all the data types of its generic type; typed forms are
available as `NAME_TYPE` for ABS, SQRT, LN, LOG, EXP, SIN, COS, TAN, ASIN,
ACOS, ATAN, ADD, MUL, SUB, DIV, MOD, EXPT, MOVE, SHL, SHR, ROR, ROL, AND, OR,
XOR, NOT, SEL, MAX, MIN, LIMIT, GT, GE, EQ, LE, LT, NE, MULTIME and DIVTIME
(e.g. `ADD_INT`, `MULTIME_REAL`), and `MUX_*_**` (e.g. `MUX_INT_REAL`).

Type conversions supported (table 22 footnote a): `*_TO_**` between all
pairs of the numeric types (SINT, INT, DINT, LINT, USINT, UINT, UDINT,
ULINT, REAL, LREAL), bit string types (BOOL, BYTE, WORD, DWORD, LWORD),
STRING and WSTRING; between every elementary type and STRING/WSTRING;
DT_TO_TOD and DT_TO_DATE (table 30); and, as implementation-dependent
conversions (table 30 note 5), DATE_TO_DT, TOD_TO_TIME, TIME_TO_TOD and the
conversions between TIME/TIME_OF_DAY/DATE/DATE_AND_TIME and the numeric
types. The effects are specified in `implementation-parameters.md`
(2.5.1.5.1, 2.5.1.5.6).

## IL language elements

| Table No. | Feature No. | Features description |
|---|---|---|
| 51b | 1 | Parenthesized expression beginning with explicit operator |
| 51b | 2 | Parenthesized expression (short form) |
| 52 | 1 | LD (N) |
| 52 | 2 | ST (N) |
| 52 | 3 | S, R |
| 52 | 4 | AND (N, () |
| 52 | 5 | & (N, () |
| 52 | 6 | OR (N, () |
| 52 | 7 | XOR (N, () |
| 52 | 7a | NOT |
| 52 | 8 | ADD (() |
| 52 | 9 | SUB (() |
| 52 | 10 | MUL (() |
| 52 | 11 | DIV (() |
| 52 | 11a | MOD (() |
| 52 | 12 | GT (() |
| 52 | 13 | GE (() |
| 52 | 14 | EQ (() |
| 52 | 15 | NE (() |
| 52 | 16 | LE (() |
| 52 | 17 | LT (() |
| 52 | 18 | JMP (C, N) |
| 52 | 19 | CAL (C, N) |
| 52 | 20 | RET (C, N) |
| 52 | 21 | ) |
| 53 | 1a | CAL of function block with non-formal argument list |
| 53 | 1b | CAL of function block with formal argument list |
| 53 | 2 | CAL of function block with load/store of arguments |
| 53 | 3 | Use of function block input operators |
| 53 | 4 | Function invocation with formal argument list |
| 53 | 5 | Function invocation with non-formal argument list |
| 54 | 4 | S1, R (SR) |
| 54 | 5 | S, R1 (RS) |
| 54 | 6 | CLK (R_TRIG, F_TRIG) |
| 54 | 8 | CU, R, PV (CTU) |
| 54 | 9 | CD, PV (CTD) |
| 54 | 10 | CU, CD, R, PV (CTUD) |
| 54 | 11 | IN, PT (TP) |
| 54 | 12 | IN, PT (TON) |
| 54 | 13 | IN, PT (TOF) |

## ST language elements

| Table No. | Feature No. | Features description |
|---|---|---|
| 55 | 1 | Parenthesization |
| 55 | 2 | Function evaluation |
| 55 | 3 | Exponentiation ** |
| 55 | 4 | Negation - |
| 55 | 5 | Complement NOT |
| 55 | 6 | Multiply * |
| 55 | 7 | Divide / |
| 55 | 8 | Modulo MOD |
| 55 | 9 | Add + |
| 55 | 10 | Subtract - |
| 55 | 11 | Comparison <, >, <=, >= |
| 55 | 12 | Equality = |
| 55 | 13 | Inequality <> |
| 55 | 14 | Boolean AND & |
| 55 | 15 | Boolean AND AND |
| 55 | 16 | Boolean Exclusive OR XOR |
| 55 | 17 | Boolean OR OR |
| 56 | 1 | Assignment |
| 56 | 2 | Function block invocation and FB output usage |
| 56 | 3 | RETURN |
| 56 | 4 | IF |
| 56 | 5 | CASE |
| 56 | 6 | FOR |
| 56 | 7 | WHILE |
| 56 | 8 | REPEAT |
| 56 | 9 | EXIT |
| 56 | 10 | Empty statement |

## Graphical languages

Graphical elements are written in the semigraphic form with the
characters of ISO/IEC 10646-1 (the odd-numbered features of table 57). They
can be used for the bodies of program organization units, actions and
transitions, for the graphical declaration of the external interface of
program organization units (figures 6 b, 11, 12 a), and inside RESOURCE
declarations for tasks and their association with programs and function
blocks (table 50); `structure fmt` writes the latter in textual form.

| Table No. | Feature No. | Features description |
|---|---|---|
| 57 | 1 | Horizontal lines: ISO/IEC 10646-1 "minus" character |
| 57 | 3 | Vertical lines: ISO/IEC 10646-1 "vertical line" character |
| 57 | 5 | Horizontal/vertical connection: ISO/IEC 10646-1 "plus" character |
| 57 | 7 | Line crossings without connection: ISO/IEC 10646-1 characters |
| 57 | 9 | Connected and non-connected corners: ISO/IEC 10646-1 characters |
| 57 | 11 | Blocks with connecting lines: ISO/IEC 10646-1 characters |
| 57 | 13 | Connectors using ISO/IEC 10646-1 characters |
| 58 | 1 | Unconditional jump (FBD) |
| 58 | 2 | Unconditional jump (LD) |
| 58 | 3 | Conditional jump (FBD) |
| 58 | 4 | Conditional jump (LD) |
| 58 | 5 | Conditional return (LD) |
| 58 | 6 | Conditional return (FBD) |
| 58 | 7 | Unconditional return from FUNCTION or FUNCTION_BLOCK |
| 58 | 8 | Unconditional return: alternative representation in LD |
| 59 | 1 | Left power rail |
| 59 | 2 | Right power rail |
| 60 | 1 | Horizontal link |
| 60 | 2 | Vertical link |
| 61 | 1 | Normally open contact `--\| \|--` |
| 61 | 2 | Normally open contact `--! !--` |
| 61 | 3 | Normally closed contact `--\|/\|--` |
| 61 | 4 | Normally closed contact `--!/!--` |
| 61 | 5 | Positive transition-sensing contact `--\|P\|--` |
| 61 | 6 | Positive transition-sensing contact `--!P!--` |
| 61 | 7 | Negative transition-sensing contact `--\|N\|--` |
| 61 | 8 | Negative transition-sensing contact `--!N!--` |
| 62 | 1 | Coil |
| 62 | 2 | Negated coil |
| 62 | 3 | SET (latch) coil |
| 62 | 4 | RESET (unlatch) coil |
| 62 | 8 | Positive transition-sensing coil |
| 62 | 9 | Negative transition-sensing coil |

Networks are evaluated in the order of 4.1.3, 4.2.6 and 4.3.3: top to
bottom, each element once all its inputs are known; in feedback paths the
value of the feedback variable from the previous evaluation is used
(figure 23). In LD, several links connected to one vertical link form a
wired OR; in FBD, outputs connected together are an error (4.3.2).

## Accompanying documents

* Implementation-dependent parameters (1.5.1 b): `implementation-parameters.md`
  (`structure params`).
* Treatment of errors (1.5.1 c, d): `error-handling.md` (`structure errors`).
* Extensions (1.5.1 e, f): `extensions.md` (`structure extensions`); their use
  is reported, as errors with `--strict`.
* Formal syntax (1.5.1 i): `formal-syntax.md` (`structure syntax`).
* Reading and writing of library elements (1.5.1 j): all elements are read
  by every command and written by `structure fmt`; graphical bodies are
  written in their semigraphic form, graphical interface declarations in
  their textual form.
