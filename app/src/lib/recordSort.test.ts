import { describe, expect, it } from 'vitest';
import type { RecordLineView } from './bindings/RecordLineView';
import { DEFAULT_SORT, nextSort, sortRecord } from './recordSort';

const t = (games: number, wins: number, draws = 0) => ({ games, wins, losses: games - wins - draws, draws });
const line = (name: string, rating: number | null, all: ReturnType<typeof t>, gold = t(0, 0)): RecordLineView => ({
  opponentId: name,
  opponent: name,
  all,
  gold,
  silver: t(all.games - gold.games, all.wins - gold.wins),
  rating,
  ratingAt: null,
});

const rows = [
  line('carol', 1700, t(10, 5), t(6, 3)),
  line('alice', null, t(4, 4), t(1, 1)),
  line('Bob', 1500, t(4, 1, 2), t(3, 0)),
];
const names = (r: RecordLineView[]) => r.map((x) => x.opponent);

describe('sortRecord', () => {
  it('keeps the order of ties', () => {
    expect(names(sortRecord(rows, DEFAULT_SORT))).toEqual(['carol', 'alice', 'Bob']);
  });
  it('sorts names without case', () => {
    expect(names(sortRecord(rows, { key: 'opponent', descending: false }))).toEqual(['alice', 'Bob', 'carol']);
    expect(names(sortRecord(rows, { key: 'opponent', descending: true }))).toEqual(['carol', 'Bob', 'alice']);
  });
  it('puts missing ratings last either way', () => {
    expect(names(sortRecord(rows, { key: 'rating', descending: true }))).toEqual(['carol', 'Bob', 'alice']);
    expect(names(sortRecord(rows, { key: 'rating', descending: false }))).toEqual(['Bob', 'carol', 'alice']);
  });
  it('sorts by score, draws counting half', () => {
    expect(names(sortRecord(rows, { key: 'score', descending: true }))).toEqual(['alice', 'carol', 'Bob']);
  });
  it('sorts by wins, then fewer losses', () => {
    const more = [...rows, line('dave', 1600, t(9, 5))];
    expect(names(sortRecord(more, { key: 'wl', descending: true }))).toEqual(['dave', 'carol', 'alice', 'Bob']);
  });
  it('sorts by games as gold', () => {
    expect(names(sortRecord(rows, { key: 'gold', descending: true }))).toEqual(['carol', 'Bob', 'alice']);
  });
});

describe('nextSort', () => {
  it('turns the same column around', () => {
    expect(nextSort(DEFAULT_SORT, 'games')).toEqual({ key: 'games', descending: false });
  });
  it('starts names A–Z and numbers highest first', () => {
    expect(nextSort(DEFAULT_SORT, 'opponent')).toEqual({ key: 'opponent', descending: false });
    expect(nextSort(DEFAULT_SORT, 'rating')).toEqual({ key: 'rating', descending: true });
  });
});
