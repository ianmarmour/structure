(* Program AGV of annex F.8 of IEC 61131-3: an automatic guided vehicle
   travelling between a left (LEFT_LS) and a right (RIGHT_LS) limit switch.
   SINGLE_PB starts one cycle, DOUBLE_PB two; non-repeat locking holds while
   a pushbutton stays pressed.

   Graphical declaration of the program (figure F.10), body in graphical
   SFC (figure F.12) and graphical configuration (figure F.11).

   structure run examples/agv.st --for T#200ms --set %IX3=TRUE --set %IX1=TRUE \
       --every T#50ms --watch %QX1 --watch %QX2
*)
PROGRAM
       +----------------------+
       |         AGV          |
BOOL---|SINGLE_PB    FWD_MOTOR|---BOOL
BOOL---|DOUBLE_PB    REV_MOTOR|---BOOL
BOOL---|LEFT_LS               |
BOOL---|RIGHT_LS              |
       +----------------------+
VAR CYCLE : BOOL; END_VAR

+--------------------------+
|                          |
|                     +====+====+
|                     || START ||
|                     +====+====+
|                          |
|       +------------------*------------+
|       |                               |
|       + READY.X & SINGLE_PB           + READY.X & DOUBLE_PB
|       |                               |
|   +---+----+ +---+-------+       +----+-----+ +---+-------+
|   | SINGLE |-| N | CYCLE |       | DOUBLE_1 |-| N | CYCLE |
|   +---+----+ +---+-------+       +----+-----+ +---+-------+
|       |                               |
|       *---------+                     + DONE.X
|       |         |                     |
|       |         + DONE.X & DOUBLE_PB  |
|       |         +---------------------+
|       |                               |
|       + DONE.X & NOT DOUBLE_PB +------+------+
|       |                        | DOUBLE_WAIT |
|       |                        +------+------+
|       |                               |
|       |                               + READY.X
|       |                               |
|       |                          +----+-----+ +---+-------+
|       |                          | DOUBLE_2 |-| N | CYCLE |
|       |                          +----+-----+ +---+-------+
|       |                               |
|       |                               + DONE.X
|       |                               |
|       +---------------+---------------+
|                       |
|                 +-----+------+
|                 | NON_REPEAT |
|                 +-----+------+
|                       |
|                       +NOT(SINGLE_PB OR DOUBLE_PB)
|                       |
+-----------------------+


+-----------+
|           |
|      +====+====+
|      || READY ||
|      +====+====+
|           |
|           + CYCLE
|           |
|      +----+----+   +---+-----------+
|      | FORWARD |---| N | FWD_MOTOR |
|      +----+----+   +---+-----------+
|           |
|           + RIGHT_LS
|           |
|      +----+----+   +---+-----------+
|      | REVERSE |---| N | REV_MOTOR |
|      +----+----+   +---+-----------+
|           |
|           + LEFT_LS
|           |
|       +---+---+
|       | DONE  |
|       +---+---+
|           |
|           + NOT CYCLE
|           |
+-----------+
END_PROGRAM

CONFIGURATION AGV_CONTROL
  RESOURCE AGV_PROC ON SMALL_PC
             AGV_1
          +----------------------+
          |         AGV          |
  %IX1----|SINGLE_PB    FWD_MOTOR|---%QX1
  %IX2----|DOUBLE_PB    REV_MOTOR|---%QX2
  %IX3----|LEFT_LS               |
  %IX4----|RIGHT_LS              |
          +----------------------+
  END_RESOURCE
END_CONFIGURATION
