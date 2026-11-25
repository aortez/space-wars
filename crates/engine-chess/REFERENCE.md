# Chess rules reference provenance and reproduction

`engine-chess` is a Rust-native rules model. It does not build, link or ship
another chess program. The reference suite in `tests/rules` compares the model
with expected values held in tables in the test code, and this page records
where those values come from and how they were checked.

The crate also holds the search that computer opponents play by. Its numbers,
its tests and its measurements are under [The search](#the-search).

## Tables

| Module in `tests/rules` | Its table states |
| --- | --- |
| `perft.rs` | How many sequences of 1, 2, 3 ... legal half-moves start from a position |
| `legal_moves.rs` | Every legal move in a position |
| `outcomes.rs` | Whether a position ends the game, why, and whether it is check |
| `games.rs` | Results, draw claims and move rights after a sequence of moves |

The tables are written by tooling kept outside the repository, on the owner's
workstation, under the name `space-wars-chess-reference`. The tooling relies on
GPL libraries and is not needed to build or test this crate; the last section
says what it does.

## Sources

These were consulted on 2026-11-23.

- [FIDE Laws of Chess taking effect from 1 January 2023](https://handbook.fide.com/chapter/e012023),
  Articles 3.7 to 3.10, 5.1, 5.2, 9.2, 9.3 and 9.6. They decide what a case
  expects wherever the points below say so.
- [Chess Programming Wiki, Perft Results](https://www.chessprogramming.org/Perft_Results),
  for the initial position and its positions 2 to 6, including the
  colour-swapped form of position 4. The page was being served as a read-only
  recovery copy of the wiki.
- [Chess Perft test positions](https://gist.github.com/peterellisjones/8c46c28141c162d1d8a0f0badbc9cff9),
  a gist by Peter Ellis Jones, at revision
  `b11af5a3dd978724ac2d4a531217133b9a3ad9ba`. It lists 23 positions with one
  depth and count each and names no earlier author. Two repeat wiki positions
  5 and 6; the other 21 are in `perft.rs`, with the shallower counts added.
- The owner's earlier engine,
  [`aortez/cuda-chess@9591009`](https://github.com/aortez/cuda-chess/tree/9591009cdf4948d17740b27810cbca6326d2e711).
  `king-may-not-step-into-attack` is the position from its `TestLegalMoveGen`.
  Its `TestRegressions` checks the en passant square after a two-square
  advance; the en passant cases in `games.rs` check instead that the capture
  becomes legal. The castling cases in `legal_moves.rs` follow the table in
  its `TestCastleMoves`, which moves an attacking rook along the far rank one
  file at a time. They are rewritten with both kings on the board, because the
  original positions have one king each.

Reference pages can change. The expected values are reviewed changes to the
tables and are never fetched while testing.

## How the values were derived

Three chess libraries by two authors computed the values. None is a dependency
of this repository.

| Library | Version | Licence | Used for |
| --- | --- | --- | --- |
| [python-chess](https://github.com/niklasf/python-chess) | 1.11.2 | GPL-3.0-or-later | Every value except the eight largest perft counts |
| [shakmaty](https://github.com/niklasf/shakmaty) | 0.30.0 | GPL-3.0-or-later | Every perft count, and a run of the whole suite |
| [cozy-chess](https://github.com/analog-hors/cozy-chess) | 0.3.4 | MIT | Every perft count |

- **Perft.** shakmaty and cozy-chess computed all 123 counts and agree on each.
  python-chess recomputed the 115 counts up to 5,000,000, which is 32,150,640
  move sequences, and agrees. Every count the two published sources print for
  these positions and depths equals the computed one: 33 from the wiki, whose
  table for position 4 also covers its colour-swapped form, and 21 from the
  gist.
- **Legal moves and outcomes.** Computed with python-chess. Each case was also
  written with the moves it is meant to allow and forbid, or the result it is
  meant to have, and was only kept when python-chess agreed.
- **Games.** Computed with python-chess from its repetition test, half-move
  clock and material test, and checked against its own
  `can_claim_threefold_repetition` and `can_claim_fifty_moves`.

## What the cases take the rules to mean

- **Same position.** Two positions are the same when the same player is to
  move, the same pieces stand on the same squares, both players have the same
  castling rights and the same en passant capture is legal. Article 9.2.3 asks
  that "the possible moves of all the pieces of both players are the same". A
  two-square pawn advance therefore starts a new position only when a pawn can
  legally capture it en passant.
- **Claims.** The claims a case expects as the position stands follow Articles
  9.2.2 and 9.3.2: the position has appeared at least three times, or 100
  half-moves have passed without a capture or pawn move. The moves it expects
  to support a claim follow 9.2.1 and 9.3.1: the same would be true once the
  named move is played. A claimable draw does not end the game.
- **Automatic draws.** Article 9.6: the fifth appearance of a position, or 150
  half-moves without a capture or pawn move. "If the last move resulted in
  checkmate, that shall take precedence."
- **Dead position.** Article 5.2.2 draws the game when "neither player can
  checkmate the opponent’s king with any series of legal moves". The cases
  expect that result for three kinds of material only: king against king; king
  and one bishop or one knight against king; and kings with any number of
  bishops that all stand on squares of one colour. Blocked positions that are
  dead for other reasons appear in no case.
- **Perft.** Only the rules of movement apply. A sequence continues through
  positions where a draw rule would have ended the game.

Two situations are left out because the Laws do not settle them. No case
expects a result where two draw rules apply at once, since their order is not
defined. No case expects a claim that names a move which itself ends the game.

## Checking the suite itself

On 2026-11-23 the suite ran against shakmaty in place of this crate. `main.rs`
and the four modules with tables were copied unchanged, and `engine.rs` was
replaced by one that answers with shakmaty. Repetition and claims were written
for that run on top of shakmaty's positions, separately from python-chess.

- Every test passed, including the eight perft counts above 5,000,000, which
  took 7.6 seconds in a release build.
- The same run with one rule deliberately broken failed at least one test for
  each of 26 breaks:

| Rule broken | Breaks tried | Modules that noticed |
| --- | --- | --- |
| Move generation: no underpromotion, no en passant, no castling | 3 | `perft.rs`, `legal_moves.rs`, `games.rs` |
| Castling allowed whenever the right remains, with rights that never lapse, or with a right that outlives the rook's capture | 3 | `legal_moves.rs`, `games.rs` |
| Same position: ignoring side to move, castling rights or en passant, counting every two-square advance, counting an illegal en passant capture, comparing placement alone | 6 | `games.rs` |
| History that keeps only the last 16 or 64 positions | 2 | `games.rs` |
| Dead position: bare kings only, never, any bishops, two knights | 4 | `outcomes.rs`, `games.rs` |
| Automatic draws: none at five or at 75, a draw at three or at 50, 75 moves outranking checkmate | 5 | `games.rs` |
| Claims: the fifty-move count one early or one late, the named move ignored | 3 | `games.rs` |

That run could remove legal moves but not add illegal ones, apart from
castling, so the suite's ability to notice a move generator that allows too
much rests on the perft counts and the complete move sets.

## Checking the model

On 2026-11-23 the model and shakmaty played 3,300 random games from ten
starting positions, with moves often taken back so that positions repeat.
After every half-move the two had to agree on the position written as FEN, the
number of sequences of two half-moves, the result, check, the legal moves and
both kinds of draw claim.

- They agreed on all 420,882 positions. 107 games ended in checkmate, 19 in
  stalemate, 752 in a dead position, 313 at the fifth repetition and 362 at 75
  moves; the other 1,747 were stopped after 160 half-moves. 34,825 positions
  had a draw to claim.
- One difference is deliberate and the comparison allows for it: the model
  accepts no claim that names a move which itself ends the game.
- The model passes every perft count in the suite. The eight above 5,000,000
  take about 26 seconds in a release build on the development workstation.

## What the suite does not cover

- The model's own interface: rejecting stale, illegal, out-of-turn and
  post-result actions without changing state, resignation, revisions, and
  restoring a snapshot. `tests/game.rs` covers those.
- What the tables cannot see from outside, such as a position's hash staying
  equal to one worked out from nothing. `src/tests.rs` covers those.
- Computer play, which has tests of its own: see [The search](#the-search).

## The search

`src/search` chooses the action of a computer opponent. It is the usual
search for chess: all moves to a depth that grows by one half-move at a time,
then only captures that do not lose material. It skips lines that cannot
change the result, and gives up early on lines that are unlikely to.

### Where the evaluation comes from

The piece values (100, 330, 350, 520 and 980 centipawns) and the seven tables
of bonuses by square are those of
[aortez/cuda-chess](https://github.com/aortez/cuda-chess) at commit
`9591009cdf4948d17740b27810cbca6326d2e711`, in `src/Chess_Constants.h` and
`src/PieceSquareTables.cpp`. Two things differ from what that engine computed.

- Its knight table lacks the comma that ends the first row. The row's last
  value and the next row's first were compiled as one number, -16, and every
  later value applied one square early. The table here is the one as written.
- It read every table for White by turning the board half a turn, which also
  exchanges the a-file with the h-file. One table is not the same from left
  to right: the king's while the board is full. White's king was therefore
  scored as if on the mirrored file, -30 at home on e1 and +30 on f1. Here
  both players read the files as written.

That engine's other terms are not used yet: passed pawns, pawn structure, the
bishop pair, mobility, and rooks on open files and on the seventh rank.

### What the search is compared with

`src/search/tests.rs` holds a second search that tries every move to the
depth, with no window, no table and no ordering of moves. Most of what the
real search does to save work is meant to leave the answer unchanged, so the
two must agree. For each of 17 positions the real search has to give every
legal move the plain search's value if the move is within the margin of the
best, and a ceiling on it otherwise.

Past the full search, both leave out captures that lose material in the
exchange on their square. Each works the exchange out its own way: the real
search with sets of squares, in `src/search/exchange.rs`, and the plain one a
square at a time on a plain board. The two are also compared on every legal
move met in thirty random games, more than 20,000 moves in all, and on
fourteen exchanges worked out by hand.

Four things change answers by design, and each is switched off in the runs it
would affect.

- A table entry kept from a deeper search stands in for a shallower one. The
  answer is then better than the plain search's, not equal to it.
- With the table in use, a position is given the value it had when first
  reached. A repeated position is a draw only on some ways of reaching it,
  so the table can pass on a value that another path would not give.
- A player may pass. If the position is still good enough after handing the
  move over, it is taken to be good enough, and none of its moves is tried.
- The reply to a quiet move tried late is searched less deeply than its
  turn. Only if the move then looks better than the best so far is it
  searched again in full.

Nothing can show the answers of the last two right, since giving up on a
line is the point of them. They are tested in four other ways.

- **When each may happen:** the conditions are small functions with tests of
  their own. A player may not pass in check, twice running, with pawns and a
  king only, or where a value is wanted and not a bound. The first three
  moves of a position are never searched shallow, nor is a capture, a
  promotion or a move that meets or gives check. A shallow search may only
  reject a move.
- **That a search in progress keeps to them:** after every step of searches
  of eight positions, a test walks the stack and checks each condition
  against the positions actually there.
- **That nothing else changes:** a search too shallow for either agrees with
  the plain search exactly, and so does a search of a pawn ending with
  passing allowed, where nobody has a piece to pass with.
- **That a mate is real:** with both in use, an announced mate is no
  quicker than the quickest, and is delivered when the game is played out.

Whether they make the engine play better is for [the
benchmark](#the-benchmark) to say.

`tests/search.rs` covers what a host relies on: a legal action after any
number of steps, the same decision for the same game, settings, seed and
steps, the budget and memory limits, mates, stalemate, and draw claims taken
by the player who is losing and declined by the one who is winning.

### Checking the tests

On 2026-11-24 the search was broken in 91 ways, one at a time, in a copy of
the crate, and its tests were rerun. Every break failed at least one test.

The sweep grew with the search. A first run of 42 breaks had missed seven,
and a test was added for each. Of the 43 added with the exchange, the pass,
the reductions and the ordering of quiet moves, the tests at first missed
one: captures given marks that only quiet moves should have.

| What was broken | Breaks | Tests that noticed |
| --- | --- | --- |
| Checkmate without its distance, stalemate as mate, a mate missed at the last position searched | 3 | Comparison with the plain search, mate distance |
| The fifty-move rule at 50 half-moves, never, or ahead of checkmate | 3 | Comparison with the plain search |
| Repetition never, at the first return to the root, only after three earlier occurrences, without the game's own positions | 4 | Positions already repeated in the game |
| Dead material ignored, illegal moves searched | 2 | Comparison with the plain search |
| A narrow window not followed by a wider search, or turned the wrong way, at the root or below it | 3 | Comparison with the plain search |
| Captures past the full search: standing pat in check, at the wrong bound or not at all; one half-move too deep; lesser promotions or quiet moves followed; promotions or en passant left out | 8 | Comparison with the plain search |
| Captures that lose material followed past the full search, or tried before quiet moves | 2 | Comparison with the plain search, order of moves |
| The exchange on a square: every capture thought sound, a piece behind another missed, a king taking what is defended, no stopping while ahead, the pawn taken en passant left standing, a promotion not counted, the dearest piece or the higher square capturing first | 8 | Exchanges worked out by hand, the exchange on a plain board |
| The table: bounds exchanged, any depth accepted, mate distance not rebased, the key unchecked, the sign of the score lost, no answer at all | 9 | The table against the plain search, the same mates with and without it, its own unit tests |
| The table's move tried last, captures ordered backwards | 2 | Order of moves |
| A pass in check, twice running, with pawns only, where a value is wanted, too near the end of the search, from a position that looks too poor, or about a mate | 7 | When a player may pass, the walk over searches in progress |
| A mate kept from a pass, a pass that proves anything, its reply searched as deep as a move's or about the wrong bound | 4 | What a pass proves, the walk over searches in progress |
| The pass itself: the en passant square, the hash or the player left as they were; pieces counted for both players | 4 | `src/tests.rs` |
| A shallow search of a capture, of a move in check, of a check, of one of the first moves, too near the end of the search, or leaving only captures | 6 | How much a search is reduced, the walk over searches in progress |
| A shallow search taken at its word, or repeated no deeper | 2 | What a shallow search may decide, the walk over searches in progress |
| The marks on quiet moves: every move marked up or down, captures marked, nothing marked, every depth counting alike, the marks ignored, past their limit, or shared by both players | 8 | Marks on quiet moves, order of moves |
| Illegal moves counted as tried, lesser promotions tried before quiet moves | 2 | The walk over searches in progress, order of moves |
| Choice among moves: the margin one point out, a margin kept beside a mate, ceilings taken for values, the seed ignored | 4 | Comparison with the plain search, `tests/search.rs` |
| A draw always claimed, never claimed, claimed before anything was searched | 3 | `tests/search.rs` |
| The budget ignored, a depth started without the budget to finish it, any mate ending the search | 3 | Budget, mate distance |
| A decision that names the wrong state of the game | 1 | A legal action after any step |
| Evaluation for the wrong player, tables read from the far side, tables mirrored for White | 3 | Evaluation |

A break that changes only the order in which moves are tried changes no
answer. The sweep includes those that reverse an order the tests state, and
leaves the rest out.

The sweep cannot say that the pass and the reductions are tuned well, only
that they keep to their conditions. A break that merely weakens them, such
as a pass tried a half-move later, is for the benchmark to notice.

### How strong the levels are

On 2026-11-24 the tooling outside the repository measured the three levels
with their starting budgets, once with the first search and again after the
changes listed under [the benchmark](#the-benchmark). The figures here are
the later ones. Nothing here is a rating, and a level that wins every game
against a weaker level of the same search need not be as far ahead of a
person.

- **Test positions:** the earlier engine's repository holds `WAC.EPD`, 300
  positions from *Win at Chess* with the move to find in each. Searching
  without the margin, Easy found 36 (12%), Normal 223 (74%) and Hard 263
  (88%). Hard found 280 (93%) with four times its budget and 285 (95%) with
  sixteen times. The first search had found 36, 182 and 222, and 250 and 258
  with the larger budgets.
- **Games:** each pair of levels played 50 openings of four random half-moves,
  each opening twice with the colours exchanged. Normal beat Easy in all 100
  games, and so did Hard. Hard against Normal won 97 and drew 3.
- **A level against itself:** over the same 100 games, about four in five
  ended in checkmate and the rest in a claimed draw or a dead position. The
  median game lasted 122 half-moves at Easy, 194 at Normal and 198 at Hard.
  Four games at Easy and six at Normal were stopped undecided after 400
  half-moves.

Easy searches two half-moves deep, too shallow for a pass or a reduction, so
only the order of its moves changed. The budgets of Normal and Hard are
those the first search was given. They buy more now, and have not been set
again.

### The benchmark

`examples/uci.rs` is a UCI front end for the search, so that standard chess
tools can run it. The game does not use it. The tooling outside the
repository uses it to give the engine a rating that can be measured again
after every change:

```sh
/path/to/space-wars-chess-reference/benchmark.sh /path/to/space-wars
```

- **What is played:** the engine, at 100,000 positions a move, plays 100
  openings twice each against each of seven fixed opponents, and then against
  the baseline: a kept copy of the search as it was first built, before any
  of the changes below.
- **Why it repeats:** the openings come from a fixed seed, the engine looks
  at a fixed number of positions, and the opponents are Stockfish 19 at full
  strength held to 100, 200, 300, 500, 1,000, 2,000 and 5,000 positions a
  move, which plays the same game every time. Two runs of one engine give
  the same output to the last figure.
- **The scale:** the opponents' ratings were fitted once, to 2,100 games
  against the baseline, and are fixed. The scale is pinned so that the
  baseline at 100,000 positions a move is 1750. It is this field's scale and
  no rating list's.
- **What it leaves out:** speed. An engine that looks at the same positions
  more slowly rates the same, so the timings below are measured separately.

On 2026-11-24 the baseline rated 1592 at 10,000 positions a move, 1752 at
100,000 and 1924 at 1,000,000, each give or take about 30. At 100,000 it
played level with Stockfish at one or two hundred positions a move.

Stockfish can also be held to a rating of its own, and a ladder of such
opponents put the baseline at 1563, 1744 and 2002 for the same three
budgets, give or take 57. That measurement cannot be repeated: Stockfish
chooses its weaker moves by a random sequence started from the clock. Its
ratings are also wider apart than its play. Held to ratings 200 apart, it
beat itself by 63 to 111 points.

In about 13,000 games against Stockfish no game ended on a move that this
crate's rules refused.

#### What each change was worth

Each was measured on 2026-11-24 at 100,000 positions a move.

| The search | Rating | Against the baseline, in 200 games |
| --- | --- | --- |
| As first built | 1752 ± 25 | |
| With losing captures left out past the full search | 1781 ± 24 | +37 (-7 to +82) |
| And a pass, where three half-moves of search or more are left | 1795 ± 24 | +87 (+44 to +132) |
| And the pass from two half-moves, quiet moves ordered by their marks, and late moves searched less deeply | 1863 ± 23 | +212 (+164 to +270) |

With all of them the engine rated 1672 at 10,000 positions a move and 2036
at 1,000,000, each give or take about 30. That is 80 to 110 above the
baseline at every budget. At 100,000 it plays level with Stockfish at about
300 positions a move.

Two builds of one engine beat each other by more than either gains against
other programs: the 212 points won from the baseline were 111 on the field's
scale.

The settings were chosen by matches of 600 games between builds, at 100,000
positions a move.

- **The pass:** allowed where two half-moves of search are left, it beat
  three by 42 (17 to 67), and one lost 21 to three. Searching its reply a
  further half-move shallower in deep searches made no difference that
  showed.
- **The reductions:** by the formula in `reduction_for` they won 140 (114 to
  167) from a search without them. A flat reduction of one half-move won 86,
  and the formula sparing five moves and not three won 84. A formula
  two-thirds as steep won 132, which is no difference.
- **The order of quiet moves:** by their marks, with nothing searched less
  deeply, it won 20 (-5 to +45).
- **The table:** telling it what a pass had proved saved no positions, and
  was taken out.

#### Measured and not adopted

A search stops deepening when the positions it has left would not finish
another depth. On five test positions it used 31% to 69% of a budget of
100,000. Ways of using the rest were played against the engine as it is, in
600 games each, and none has been adopted.

- **Always going on:** starting the next depth whenever positions remain,
  and taking the best move of the depth left unfinished, won 101 (75 to
  128). Every search then uses its whole budget, about twice the positions
  it uses now.
- **Keeping what an unfinished depth found:** taking its best move only
  when a depth was started and ran out won 26 (1 to 50), for no more work.
- **Starting sooner:** a depth started when the budget holds twice the
  positions of the last, and not three times, won 31. Once won 40.

### Measurements

On 2026-11-24 `cargo run --release -p engine-chess --example search_bench`
ran on the development workstation and, built for `aarch64`, on two of the
kiosks while they showed the Clock. Every machine reached the same decision
from the same number of positions in all fifteen searches. The search was
then the baseline of [the benchmark](#the-benchmark), before the changes
listed there.

| | Workstation | Pi 5 (`spacewars`) | Pi 4 (`sw-picade-2`) |
| --- | --- | --- | --- |
| Move generation, million positions a second | 43.5 | 14.9 | 6.1 |
| Search, million positions a second | 4.4 to 6.6 | 2.1 to 2.7 | 0.9 to 1.2 |
| Normal, four ordinary positions | 7 to 11 ms | 15 to 27 ms | 34 to 63 ms |
| Hard, the same four | 84 to 116 ms | 188 to 295 ms | 429 to 673 ms |
| Starting a search | under 0.1 ms | under 0.3 ms | under 0.8 ms |
| Slowest of the first 20,000 steps | 9 µs | 39 µs | 497 µs |

- A host can stop a search after any step, so the slowest step is how long
  stopping takes.
- Easy looks at fewer than a thousand positions and answers in under a
  millisecond everywhere.
- Normal used 39,000 to 74,000 of its 100,000 positions and searched 4 or 5
  half-moves deep in full, 7 in a rook endgame. Hard used 431,000 to 755,000
  of its 1,000,000 and searched 6 or 7 deep, 10 in the endgame.
- A search holds 88 KiB at Easy, 1.1 MiB at Normal and 16.1 MiB at Hard.
- The fifth position has twenty queens and nothing but captures. Normal's
  budget runs out there before one half-move is searched in full, in 117 ms
  on the Pi 4, and the search answers with its first legal move. Hard takes
  1.06 s there.

#### After the changes

The same program ran again on the workstation later that day, with the pass,
the reductions and the rest in the search. The workstation was busy with
other work, so the program was run in turn with one built from the baseline,
eight times each, and the quickest run of each is given. The kiosks have not
been timed again.

| Four ordinary positions | Baseline | After the changes |
| --- | --- | --- |
| Normal, half-moves searched in full | 4 to 7 | 5 to 9 |
| Normal, time | 7 to 14 ms | 6 to 24 ms |
| Hard, half-moves searched in full | 6 to 10 | 8 to 14 |
| Hard, time | 98 to 148 ms | 102 to 226 ms |
| Search, million positions a second | 3.9 to 5.6 | 2.5 to 5.8 |

- The busy workstation generated moves at 37 million positions a second, not
  43.5, and both columns are slower by about that much than they would be on
  an idle one.
- The search now looks at between 0.65 and 1.08 times as many positions a
  second as the baseline, fewest in crowded middlegames. Why has not been
  measured. More of its positions are searched in full, which costs more
  than following captures, and each of those is also evaluated to see
  whether to pass.
- The rating counts positions and not time. At equal time the gain is
  smaller than the ratings say, by an estimated 30 points: a third fewer
  positions a second is half a doubling of the budget, and a doubling is
  worth about 55.
- Normal used 36,000 to 90,000 of its 100,000 positions, and Hard 411,000 to
  568,000 of its 1,000,000.
- A search holds 104 KiB at Easy, 1.1 MiB at Normal and 16.1 MiB at Hard.
  The 16 KiB more than before hold the marks on quiet moves.
- In the position with twenty queens, Normal now searches one half-move in
  full within its budget, and Hard uses all of its budget to search two.

## The tooling outside the repository

`space-wars-chess-reference` holds eight things, with its own notes on
running them.

- A script that lists every case with its comment and what it is meant to
  show, computes the expected values with python-chess and rewrites the four
  tables. Running it again without changing a case leaves the tables as they
  are, which is how to confirm they still match the library. To add a case,
  add it there and review the change it makes to the table.
- A program that computes every perft count with shakmaty and with cozy-chess
  and fails when they disagree.
- The run of the suite against shakmaty described above, with the 26 breaks.
- The comparison of the model with shakmaty on random games described above.
- The script that breaks the search one way at a time, described above.
- A program that scores the levels of the search on a file of test positions,
  plays seeded pairs of games between them, and plays matches between any
  engines that speak UCI.
- The benchmark described above: its script, its field of opponents with
  their ratings, the baseline engine with its sources, every build rated
  since, a copy of Stockfish 19, and the log of ratings measured so far.
- Two scripts for trying a setting before keeping it. One builds the engine
  with pieces of its source replaced. The other shows how many positions
  each build needs to finish each depth.

Without the tooling, a new case can still be added by hand: write the entry in
the table and compute its expected value with any trusted chess library.
