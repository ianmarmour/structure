# Extensions (1.5.1 e)

The following features are accepted by this system but are prohibited or not
specified in IEC 61131-3:2003. They are described as *extensions to the
language as defined in IEC 61131-3*. Every use of an extension is reported
(as a warning, or as an error with `--strict`, 1.5.1 f), so programs can be
checked for program compliance (1.5.2).

## Extensions to the common elements

1. **Implicit widening conversions.** A value can be used where a value of a
   different elementary type is expected when the conversion cannot lose
   information: signed integer to a wider signed integer; unsigned integer to
   a wider unsigned or signed integer; bit string to a wider bit string;
   SINT, INT, USINT, UINT to REAL; integers of up to 32 bits to LREAL; REAL to
   LREAL. The standard requires explicit conversion functions (2.5.1.4); the
   annex F examples (e.g. F.6.3 `SUM/N` with REAL and INT) rely on such
   conversions.
2. **Integer literals as real values.** An integer literal can be used where a
   REAL or LREAL value is expected (`X := 1;` for a REAL `X`).
3. **Constant expressions in declarations.** Array bounds, subrange limits,
   string lengths, repetition factors and initial values can be constant
   expressions (e.g. `ARRAY [0..2*8-1]`) instead of literals (B.1.2, B.1.3.3).
4. **Characters outside ISO 646 in identifiers.** Letters of other scripts can
   be used in identifiers (2.1.1 notes this as a typical extension). Source
   files are read as UTF-8, or as ISO 8859-1 when they are not valid UTF-8.
5. **Parentheses for array bounds and string lengths:** `ARRAY (1..2) OF INT`
   (as written in table 18 feature 7) and `STRING(10)`.
6. **Nested anonymous structures** inside STRUCT declarations.
7. **Arrays of function block instances.**
7a. **Initial values in VAR_TEMP declarations** (temporaries are
   initialised at every invocation).

## Extensions to the ST language

8. **Missing semicolon after compound statements.** The `;` after `END_IF`,
   `END_CASE`, `END_FOR`, `END_WHILE` and `END_REPEAT` may be omitted (as in
   figure 22).
9. **Function invocation as a statement.** A function can be invoked as a
   statement, discarding its result (B.3.2.2 allows only function block
   invocations).

## Extensions to the IL language

10. **Input operators for any function block.** The input operators of
    table 54 (S1, R, S, R1, CLK, CU, CD, PV, IN, PT) can be used with any
    function block that has an input of that name.

## Extensions to the standard library

11. **Additional typed counters:** `CTUD_UDINT` (table 36 defines the other
    unsigned variants).
12. **Generic BCD conversions** `BCD_TO_**` and `**_TO_BCD` (e.g.
    `BCD_TO_INT`, `INT_TO_BCD`), used in the examples of tables 56 and F.1,
    with the bit string type inferred from the operand.

13. **RTC (real time clock)** function block of annex F.10.
14. **Signed integer destinations for BCD conversions**
    (`WORD_BCD_TO_INT`, as written in table 22 feature 3).

Conversions between time data types and other types not defined in
table 30 (e.g. `TIME_TO_REAL`, `DATE_TO_DT`) are implementation-dependent
(table 30 note 5, see annex D) rather than extensions; their use is
reported with `--report-implementation-dependent`.

## Not supported

The use of hierarchical addresses to access data in another programmable
controller (2.4.1.1), which the standard defines as an extension, is not
provided. Communication function blocks (2.5.2.3.5) are defined in
IEC 61131-5 and are not part of this implementation.
