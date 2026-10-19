# Dedale

A referee for maze bots. One or two competitor programs explore a symmetric
maze, starting from opposite corners, and race to collect its points:

- **solo**: clear the maze in as few turns as possible;
- **duo**: collect more points than the opponent.

Bots are separate programs (Python first) with a startup budget to precompute
and a time limit per move. Dedale records every game as a replay with full
timing and error analytics.

Work in progress. License: MIT.
