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
  take about 14 seconds in a release build on the development workstation.

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
change the result, and gives up early on lines that are unlikely to. It
looks at as many positions as its settings allow, and answers from the
deepest search it got into, finished or not.

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

Four more of that engine's terms are counted, with its numbers from
`src/Chess_Constants.h` and its tests for them from `src/Evaluator.cpp`.

- **A passed pawn:** 3 on its third rank, then 20, 40, 60 and 70 on its
  seventh, from the table that engine kept for them. A pawn is passed if no
  pawn of the other player stands ahead of it on its file or on a file
  beside it. Of two on a file, only the one in front can be.
- **A doubled pawn:** 11 off for a pawn that stands behind another of its
  own.
- **A rook on an open file:** 9 where the file holds no pawn of either
  player.
- **A rook on the seventh rank:** 17 while the other player's king stands
  on the eighth or a pawn of theirs on the seventh. That engine asked
  whether a pawn there was undefended, which takes every attack on the
  board.

Each was kept because it won games, and the others were not: see [the
terms of the evaluation](#the-terms-of-the-evaluation).

- **Tried and left out:** the pair of bishops (42), a pawn that another
  defends (10), a queen off its square while the board is full (50 off),
  and the squares a knight, bishop or rook could move to (5, 1 and 1 each).
- **Comes to nothing:** 10 off for a pawn with a pawn of the other player
  straight ahead of it. Each of the two blocks the other, so both players
  always have as many.
- **Not tried:** the pawns beside the king, castling, the squares round the
  other king, and checks. Most take every attack on the board.

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

Three more things are held to, each with tests of its own.

- **The fifty-move rule and the table:** the same position is worth less to
  the player ahead the nearer the hundredth quiet half-move is. From the
  count where the deepest search could reach the hundredth, the table
  therefore knows a position by that count as well as by its pieces and
  rights, and what it holds at one count is not used at another. Nor is it
  believed about a forced mate that would come after the count runs out.
  Tests tell the table something untrue of a position at one count and
  search it from others, and play out a mate announced ten half-moves from
  the hundredth. In 200 endings of a queen or a rook, half of them with a
  pawn, against a bare king, begun 11 to 40 half-moves from the hundredth
  at 20,000 positions a move, the engine won 86 this way. It won 84 with a
  table that took no notice of the count, and 82 with one that kept
  nothing the rule could reach.
- **The margin beside a mate:** however wide the margin, a move that gets
  its player mated is not good enough beside one that does not. A position
  where several moves do is compared with the plain search under the widest
  margin there is.
- **More material than a game can hold:** the rules accept a position set
  up with sixty queens. Whatever a position holds, its evaluation stops
  short of the scores kept for forced mates, and boards of more than thirty
  queens are searched at every level.

A search ends when its positions are spent, which is nearly always partway
through a depth. It answers from the moves that depth had got to, and from
the depth before while none of them is finished. After every step of
searches of the 17 positions, a test works out from the plain search what
the answer should be for exactly the moves searched so far, and compares.

Some of the search is written for speed in ways that must not change what
it finds. Each has a check of its own.

- **Sliding pieces:** the squares a rook or a bishop attacks are read from
  tables of the squares that lie each way from each square. A test compares
  them with a fill made one step at a time, for every square on 2,002
  boards.
- **The count of pieces:** the evaluation keeps its count of material and
  placement up from one position to the next. In a test build, the count
  is checked against a recount from nothing wherever a position is
  evaluated.
- **Leaving the king attacked:** a move is tested for that only if it
  could do it: made in check, by the king, en passant, or by a piece that
  shares a line with its king. The move counts of `tests/rules` are all
  made this way, so a move wrongly let through would change one of them.
  A test also makes 184,000 candidate moves both ways.
- **The exchange on a square:** a piece that takes one worth less than
  itself by more than a pawn, on a square a pawn defends, loses material
  whatever follows, and the exchange is not played out. The comparison
  with the exchange on a plain board covers it.

`tests/search.rs` covers what a host relies on: a legal action when a
search is stopped before it starts, among captures and between depths, the
same decision for the same game, settings, seed and steps, the budget and
memory limits, mates, stalemate, and draw claims taken by the player who is
losing and declined by the one who is winning. `src/search/tests.rs` asks
for a legal action after every step of three searches.

### Checking the tests

On 2026-11-25 the search was broken in 133 ways, one at a time, in a copy
of the crate, and its tests were rerun. Every break failed at least one
test.

The sweep grew with the search. A first run of 42 breaks had missed seven,
and a test was added for each. It stood at 48 when the exchange, the pass,
the reductions and the ordering of quiet moves added 43, of which the tests
at first missed one: captures given marks that only quiet moves should
have. The use of the whole budget, the work on speed and the terms for
pawns and rooks added 32 and retired one with the rule it broke, and all
were noticed.

A review of the whole change then found four things that the sweep had
never broken and no test would have noticed: the limits on a search's
settings, the range of the evaluation, the margin beside a forced mate and
the table beside the fifty-move rule. The last three were wrong as well. Ten
breaks went in with the fixes and their tests, and one more with the limit
on the positions a player may give a search.

| What was broken | Breaks | Tests that noticed |
| --- | --- | --- |
| Checkmate without its distance, stalemate as mate, a mate missed at the last position searched | 3 | Comparison with the plain search, mate distance |
| The fifty-move rule at 50 half-moves, never, or ahead of checkmate | 3 | Comparison with the plain search |
| Repetition never, at the first return to the root, only after three earlier occurrences, without the game's own positions | 4 | Positions already repeated in the game |
| Dead material ignored, illegal moves searched | 2 | Comparison with the plain search |
| A narrow window not followed by a wider search, or turned the wrong way, at the root or below it | 3 | Comparison with the plain search |
| Captures past the full search: standing pat in check, at the wrong bound or not at all; one half-move too deep; lesser promotions or quiet moves followed; promotions or en passant left out | 8 | Comparison with the plain search |
| Captures that lose material followed past the full search, or tried before quiet moves | 2 | Comparison with the plain search, order of moves |
| The exchange on a square: every capture thought sound, a piece behind another missed, a king taking what is defended, no stopping while ahead, the pawn taken en passant left standing, a promotion not counted, the dearest piece or the higher square capturing first, a pawn's defence seen from the wrong side or taken to settle every exchange, bishops left out | 11 | Exchanges worked out by hand, the exchange on a plain board |
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
| The budget ignored, a search stopped with positions left, any mate ending the search | 3 | Budget, mate distance |
| A depth left unfinished: ignored, moves not yet searched taken as searched, the moves searched left out of order; a move shown to beat the others forgotten, kept after its search ended, or scored below the best | 6 | The answer after every step against the plain search |
| Sliding pieces: a path that runs the wrong way, pieces in the way ignored, an empty path cut short, a piece on a diagonal overlooked | 4 | Sliding pieces against a fill a step at a time, `src/tests.rs` |
| A move not tested for leaving the king attacked: in check, by the king, en passant, or by a piece pinned on a diagonal or on a file | 5 | `src/tests.rs` |
| The count of pieces: pieces taken off still counted, a table read for the wrong piece, the count lost across a pass or not kept up with moves | 4 | Evaluation, the recount in a test build |
| Moves as numbers: promotions sharing one, the best move misread | 2 | `src/tests.rs`, order of moves |
| A decision that names the wrong state of the game | 1 | A legal action after any step |
| Evaluation for the wrong player, tables read from the far side, tables mirrored for White | 3 | Evaluation |
| Settings beyond their limits: a search deeper than the deepest or of no depth, captures followed past their limit, a table larger than the largest, a player given positions without limit | 5 | Limits of the settings |
| A position that looks better than a forced mate | 1 | A board full of queens |
| A mated move good enough under the widest margin | 1 | The margin beside a mate |
| The table beside the fifty-move rule: the count left out of what it knows a position by, a mate believed though it would come too late, by one half-move, or at the hundredth | 4 | The table and the count |
| Pawns and rooks: a pawn beside a passed pawn taken to stop it, or only a pawn on its file; both pawns on a file passed; doubled pawns rewarded; a file with the other player's pawn counted open; the seventh rank counted whatever stands there; Black's board not turned | 7 | Evaluation |

A break that changes only the order in which moves are tried changes no
answer. The sweep includes those that reverse an order the tests state, and
leaves the rest out.

The sweep cannot say that the pass and the reductions are tuned well, only
that they keep to their conditions. A break that merely weakens them, such
as a pass tried a half-move later, is for the benchmark to notice.

### The levels and a player's settings

A player may set all five of a search's settings, and the three levels are
settings to start from. `Settings::for_play` brings whatever a player
chooses within the limits that every search keeps, and to no more than
10,000,000 positions a move. The Pi 4 searches about a million positions a
second, and half that in the slowest position measured, so a search of
that many would take it ten to twenty seconds.

On 2026-11-25 the levels were these, with the search as it is after every
change listed under [the benchmark](#the-benchmark).

| | Easy | Normal | Hard |
| --- | --- | --- | --- |
| Deepest full search, in half-moves | 2 | 32, the deepest there is | 32 |
| Positions a move | 5,000, of which it uses under 1,000 | 10,000 | 1,000,000 |
| Half-moves of captures followed past the full search | none | 8 | 8 |
| Table | none | 1 MiB | 16 MiB |
| Margin, in centipawns | 60 | 30 | 8 |
| Plays about as well as Stockfish held to | far below 1320, its lowest | 1405 ± 58 | 2160 ± 53 |
| Test positions found, of 300 | 34 | 166 | 272 |
| A move on the Pi 4 | under 1 ms | 8 to 12 ms | 0.7 to 1.1 s |
| A move on the Pi 5 | under 1 ms | 3 to 5 ms | 0.3 to 0.45 s |
| What a search holds | 105 KiB | 1.1 MiB | 16.1 MiB |

The times are for four ordinary positions. In the slowest position measured,
which has eleven queens, a move takes about twice as long: see [On the
kiosks](#on-the-kiosks).

Until 2026-11-25 Normal had 100,000 positions a move and a margin of 20, the
budget the search was first given. With every change to the search it then
played like Stockfish held to 1725 ± 59, much nearer Hard than Easy, and
took 73 to 113 ms a move on the Pi 4. A tenth of the positions put it
between the two.

What each setting costs and buys, as far as it was measured:

- **Positions:** on Stockfish's scale, 1,000 a move played like 1175 ± 74,
  3,000 like 1308 ± 62, 10,000 like 1405 ± 58, 100,000 like 1725 ± 59 and
  1,000,000 like 2160 ± 53. That is 230 to 440 points for each tenfold. The
  fewest measured, 1,000, played far better than Easy. The time a move
  takes is in proportion to them, about a millisecond for each thousand on
  the Pi 4 and 0.4 on the Pi 5.
- **The table:** sixteen bytes an entry, and a search holds about 100 KiB
  besides. What its size is worth by itself has not been measured.
- **The margin:** moves within it of the best are all good enough, and the
  seed chooses between them. A wider margin varies the play and weakens it.
  What it costs by itself has not been measured: each setting measured here
  changed it along with the positions, from 40 at 1,000 to 8 at 1,000,000.
- **Depth and captures:** Easy is weak in another way. It looks two
  half-moves ahead and follows no captures past them, so it walks into
  exchanges. A search three half-moves deep at 5,000 positions, following
  two half-moves of captures, played like 1163 ± 75.

Each placement on Stockfish's scale comes from 60 games, 30 openings played
twice, against Stockfish 19 held to each of three or four ratings and given
200,000 positions a move. It cannot be repeated exactly: see [the
benchmark](#the-benchmark). Easy won 4 of its 180 games against Stockfish
held to 1320, 1450 and 1600, which puts it at about 765 ± 176 if the scale
goes on below Stockfish's lowest rating.

The tooling outside the repository also measured the levels against the
search itself. Nothing here is a rating, and a level that wins every game
against a weaker level of the same search need not be as far ahead of a
person.

- **Test positions:** the earlier engine's repository holds `WAC.EPD`, 300
  positions from *Win at Chess* with the move to find in each. Searching
  without the margin, Easy found 34 (11%), Normal 166 (55%) and Hard 272
  (91%). With 100,000 positions the search found 228 (76%). Hard found 286
  (95%) with four times its budget and 288 (96%) with sixteen times. The
  search as first built had found 36 at Easy, 182 at 100,000 positions and
  222 at Hard, and 250 and 258 with Hard's larger budgets.
- **Games:** each pair of levels played 50 openings of four random half-moves,
  each opening twice with the colours exchanged. Normal beat Easy in all 100
  games, and so did Hard. Hard against Normal won 99 and drew one by
  repetition.
- **A level against itself:** over the same 100 games, 94 ended in checkmate
  at Easy, 78 at Normal and 90 at Hard. Two games at Normal were stopped
  undecided after 400 half-moves, and the rest ended in a claimed draw,
  stalemate or a dead position. The median game lasted 103 half-moves at
  Easy, 159 at Normal and 164 at Hard.

Easy searches two half-moves deep, too shallow for a pass or a reduction, so
of the changes to the search only those to its evaluation and the order of
its moves reach it. A search now uses all of its positions, where at first
it used a little over half: see [How the budget is spent](#how-the-budget-is-spent).

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

In about 26,000 games against Stockfish no game ended on a move that this
crate's rules refused.

#### What each change was worth

Each was measured on 2026-11-24 at 100,000 positions a move, and the last
on 2026-11-25.

| The search | Rating | Against the baseline, in 200 games |
| --- | --- | --- |
| As first built | 1752 ± 25 | |
| With losing captures left out past the full search | 1781 ± 24 | +37 (-7 to +82) |
| And a pass, where three half-moves of search or more are left | 1795 ± 24 | +87 (+44 to +132) |
| And the pass from two half-moves, quiet moves ordered by their marks, and late moves searched less deeply | 1863 ± 23 | +212 (+164 to +270) |
| And every position of the budget used, with the answer taken from an unfinished depth | 1914 ± 23 | +319 (+262 to +394) |
| And passed and doubled pawns counted, and rooks on open files and on the seventh rank | 1957 ± 23 | +531 (+445 to +691) |
| And the table told apart by the count near the fifty-move rule | 1959 ± 23 | +531 (+445 to +691) |

With all of them the engine rated 1734 at 10,000 positions a move and 2124
at 1,000,000, each give or take about 30. That is 140 to 207 above the
baseline at every budget. At 100,000 it plays level with Stockfish at about
400 positions a move.

| Positions a move | Baseline | With the pass and the reductions | With its whole budget used | With the terms for pawns and rooks | As it is |
| --- | --- | --- | --- | --- | --- |
| 10,000 | 1592 | 1672 | 1688 | 1735 | 1734 |
| 100,000 | 1752 | 1863 | 1914 | 1957 | 1959 |
| 1,000,000 | 1924 | 2036 | 2045 | 2124 | 2124 |

The last column is the engine after a review of the whole change, which
altered its play only close to the fifty-move rule: at least a dozen of
the 3,500 games came out differently.

Two builds of one engine beat each other by more than either gains against
other programs: the 531 points won from the baseline at 100,000 positions a
move are 207 on the field's scale.

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

#### How the budget is spent

The search used to start a depth only if it had three times the positions
the last depth took, since a depth left unfinished told it nothing. Over
1,190 positions from games, the search with the pass and the reductions
looked at 57% of its budget.

It now goes on until its positions are spent, and a depth left unfinished
counts for the moves it got to. If one of those has been shown to beat what
the others must, it is played, though its own value was still being worked
out. Each was played in 600 games at 100,000 positions a move.

- **Both together:** against the search without them, they won 129 (103 to
  156).
- **For the same work:** against the search without them given 175,000
  positions, which is as many as the new one looks at on average, they won
  24 (-1 to +50). Most of the gain is in positions the earlier rule left
  unused. A search also takes much the same time now whatever the
  position.
- **The move shown to beat the others:** playing it, and not the best of
  the moves that have a value, won 50 (24 to 75) of the 129.
- **Tried first and left aside:** starting a depth with twice the positions
  of the last, and not three times, had won 31, and with as many, 40.

#### The terms of the evaluation

Each term of the earlier engine's evaluation was added by itself to the
engine as it then was, and played against it in 600 games at 100,000
positions a move.

| What was counted | Against the engine without it |
| --- | --- |
| Passed pawns | +92 (66 to 119) |
| Rooks on open files | +78 (53 to 105) |
| Doubled pawns | +43 (17 to 69) |
| Rooks on the seventh rank | +35 (10 to 61) |
| The pair of bishops | +2 (-23 to +28) |
| A pawn that another defends | -7 (-33 to +19) |
| A pawn blocked by a pawn | 0: the same games, move for move |

- **The four kept:** together they won 138 (110 to 167). They cost a tenth
  of the speed, 7.0 million positions a second against 7.7 over forty
  positions from games. With 91,200 positions against 100,000, which is the
  same time, they won 102 (76 to 129).
- **The pair of bishops and the defended pawn:** all seven terms together
  won 107 (80 to 135), and lost to the four by 49 (24 to 75). The two that
  showed nothing by themselves cost points in company.
- **Squares to move to:** added to the seven they won 85 (60 to 111), and
  as many with 4 for a bishop and 2 for a rook. Added to the four they won
  8 (-18 to +34), and they cost another sixth of the speed. With 82,500
  positions against 100,000 they lost 16 (-41 to +10). A square counted if
  it held no piece of the player's own and no pawn of the other player
  attacked it, where that engine asked that no piece attacked it.
- **A queen off its square early:** added to the seven, it lost 56 (30 to
  82).

### Measurements

Until 2026-11-25 Normal had 100,000 positions a move, and it has them in
what follows wherever it is not said to have 10,000.

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
- A search holds 88 KiB at Easy, 1.1 MiB at Normal and 16.1 MiB at Hard in
  the fifth position, which has the most moves, and a little less in the
  others.
- The fifth position has eleven queens and captures on every side. Normal's
  budget runs out there before one half-move is searched in full, in 117 ms
  on the Pi 4, and the search answers with its first legal move. Hard takes
  1.06 s there.

#### After the changes

The same program ran again on the workstation later that day, in turn with
one built from the baseline and one built from the search before it was
made quicker, eight times each. The quickest run of each is given. The last
column was timed an hour later, again in turn with the baseline, which ran
as it had before. The kiosks were timed again the day after: see [On the
kiosks](#on-the-kiosks).

| Four ordinary positions | Baseline | With the pass and the reductions | Made quicker, and using its whole budget | With the terms for pawns and rooks |
| --- | --- | --- | --- | --- |
| Move generation, million positions a second | 44.1 | 44.3 | 61.4 | 61.9 |
| Search, million positions a second | 4.4 to 6.6 | 3.0 to 5.9 | 6.5 to 10.7 | 5.7 to 9.6 |
| Normal, half-moves searched in full | 4 to 7 | 5 to 9 | 5 to 10 | 5 to 9 |
| Normal, positions | 39,000 to 74,000 | 36,000 to 90,000 | 100,000 | 100,000 |
| Normal, time | 7 to 11 ms | 6 to 20 ms | 9 to 15 ms | 11 to 18 ms |
| Hard, half-moves searched in full | 6 to 10 | 8 to 14 | 9 to 15 | 8 to 15 |
| Hard, positions | 431,000 to 755,000 | 411,000 to 568,000 | 1,000,000 | 1,000,000 |
| Hard, time | 84 to 117 ms | 81 to 187 ms | 93 to 150 ms | 104 to 167 ms |

- The pass and the reductions had cost speed: more of the positions were
  searched in full, which costs more than following captures, and the
  search looked at as few as 0.65 times as many positions a second as the
  baseline. Made quicker, it looked at 1.4 to 1.7 times as many as the
  baseline, and with the terms for pawns and rooks it looks at 1.3 to 1.5
  times as many.
- Over forty positions from games the engine played against itself, at
  1,000,000 positions each, the search went from 3.9 to 7.7 million
  positions a second. Every decision stayed the same, and so did the number
  of positions needed to search each to a fixed depth. The terms for pawns
  and rooks then took it to 7.0 million.
- A profile said where the time went: finding the next move to try took a
  fifth of it, the paths of sliding pieces an eighth, exchanges on a square
  an eighth and the evaluation a tenth. The moves and their scores are now
  kept apart and the best is found in one pass. Sliding pieces read their
  paths from tables. An exchange on a square that a pawn defends is often
  settled without being played out. The evaluation keeps its count of
  pieces from one position to the next. A move is tested for leaving the
  king attacked only where it could.
- Four more ideas were timed and dropped. Reading a table entry a step
  early, listing the few best moves apart, and a sharper test for pinned
  pieces each gained nothing. Finding the piece on a square without a
  decision at each kind was 7% slower.
- A search now takes its whole budget unless the depth limit or a forced
  mate ends it, so its time no longer varies with the position as much.
- A search holds up to 105 KiB at Easy, 1.1 MiB at Normal and 16.1 MiB at
  Hard. The 17 KiB more than at first hold the marks on quiet moves and
  each position's count of pieces.
- In the position with eleven queens, Normal searches one half-move in full
  in 29 ms, and Hard two in 302 ms.

#### On the kiosks

On 2026-11-25 the same program ran three times on each kiosk while it showed
the Clock, built for `aarch64` from the search as it is, and three times
more once Normal had 10,000 positions. The quickest run of each search is
given. Normal's earlier budget of 100,000 positions is kept in the table,
since a player may still choose it.

| | Pi 5 (`spacewars`) | Pi 4 (`sw-picade-2`) |
| --- | --- | --- |
| Move generation, million positions a second | 19.4 | 7.7 |
| Search, million positions a second | 2.2 to 3.4 | 0.9 to 1.4 |
| Normal, four ordinary positions | 3.1 to 5.0 ms | 8.2 to 12.2 ms |
| 100,000 positions, the same four | 30 to 46 ms | 73 to 113 ms |
| Hard, the same four | 304 to 447 ms | 722 to 1,098 ms |
| 10,000,000 positions, the same four | 3.0 to 4.6 s | |
| Normal, the position with eleven queens | 7.1 ms | 16.3 ms |
| 100,000 positions, the same | 78 ms | 183 ms |
| Hard, the same | 819 ms | 1,946 ms |
| 10,000,000 positions, the same | 8.0 s | |
| Starting a search | under 0.1 ms | under 0.2 ms |
| Slowest of the first 20,000 steps, in the best run | 10 µs | 33 µs |

- Every search gave the same depth, positions, move and score on the
  workstation and on both kiosks: the fifteen of this program in each of
  its runs, and eighty more over forty positions from games, to a fixed
  depth and at 100,000 positions each. Nothing in the search uses floating
  point.
- The searches of 10,000,000 positions, the most a player may give, ran
  once on the Pi 5 with Hard's other settings, by a program in the tooling
  outside the repository. The Pi 4 was not on the network by then. Each
  search looked at positions about as quickly as at 1,000,000.
- Easy answers in under a millisecond on both, and a search holds what it
  holds on the workstation.
- The kiosks look at about as many positions a second as they did with the
  baseline, where the workstation looks at 1.3 to 1.5 times as many. What
  was done for speed tells less on them, and a search that uses its whole
  budget takes one and a half to two times as long as the baseline did on
  the same four positions.

## The tooling outside the repository

`space-wars-chess-reference` holds nine things, with its own notes on
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
- What it takes to make the search quicker without changing what it finds:
  a program that searches forty positions from games and prints a digest of
  its decisions, a profiler, and a script that times builds in turn. A
  program beside them times a search at any number of positions a move.

Without the tooling, a new case can still be added by hand: write the entry in
the table and compute its expected value with any trusted chess library.
