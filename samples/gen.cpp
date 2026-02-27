/**
 * gen.cpp — Stress test generator template
 * Usage: ./gen <seed>
 *
 * Edit the generate() function at the bottom.
 * Everything above it is utility — don't touch unless needed.
 */

#include <bits/stdc++.h>
using namespace std;

mt19937 rng;
mt19937_64 rng64;

using ll = long long;
using vi = vector<int>;

void seed(int s) {
  rng.seed(s);
  rng64.seed(s);
}

// random int in [lo, hi]
int randint(int lo, int hi) {
  return uniform_int_distribution<int>(lo, hi)(rng);
}

// random long long in [lo, hi]
long long randll(long long lo, long long hi) {
  return uniform_int_distribution<long long>(lo, hi)(rng64);
}

// random int in [0, n-1]
int randindex(int n) { return randint(0, n - 1); }

// coin flip — true with probability p/q
bool chance(int p, int q) { return randint(1, q) <= p; }

// random array of `n` ints, each in [lo, hi]
vector<int> rand_array(int n, int lo, int hi) {
  vector<int> a(n);
  for (auto &x : a)
    x = randint(lo, hi);
  return a;
}

// random array of `n` long longs, each in [lo, hi]
vector<long long> rand_array_ll(int n, long long lo, long long hi) {
  vector<long long> a(n);
  for (auto &x : a)
    x = randll(lo, hi);
  return a;
}

// random permutation of [1..n]
vector<int> rand_perm(int n) {
  vector<int> p(n);
  iota(p.begin(), p.end(), 1);
  shuffle(p.begin(), p.end(), rng);
  return p;
}

// sorted random array (useful for binary search problems)
vector<int> rand_sorted(int n, int lo, int hi) {
  auto a = rand_array(n, lo, hi);
  sort(a.begin(), a.end());
  return a;
}

// array with exactly `k` distinct values
vector<int> rand_array_k_distinct(int n, int k, int lo, int hi) {
  assert(k <= hi - lo + 1);
  vector<int> pool(hi - lo + 1);
  iota(pool.begin(), pool.end(), lo);
  shuffle(pool.begin(), pool.end(), rng);
  pool.resize(k);
  vector<int> a(n);
  for (auto &x : a)
    x = pool[randindex(k)];
  return a;
}

//  STRINGS

// random lowercase string of length n
string rand_string(int n, char lo = 'a', char hi = 'z') {
  string s(n, ' ');
  for (auto &c : s)
    c = (char)randint(lo, hi);
  return s;
}

// random binary string
string rand_binary_string(int n) { return rand_string(n, '0', '1'); }

// random string from a custom alphabet, e.g. "abc"
string rand_string_from(int n, const string &alphabet) {
  string s(n, ' ');
  for (auto &c : s)
    c = alphabet[randindex(alphabet.size())];
  return s;
}

//  GRAPHS

// edges of a random tree on n nodes (1-indexed), printed as pairs
// returns list of {u, v} edges
vector<pair<int, int>> rand_tree(int n) {
  vector<pair<int, int>> edges;
  // Prüfer sequence approach
  for (int i = 2; i <= n; i++) {
    int parent = randint(1, i - 1);
    edges.push_back({parent, i});
  }
  // shuffle edge order and optionally swap u/v
  shuffle(edges.begin(), edges.end(), rng);
  for (auto &[u, v] : edges)
    if (chance(1, 2))
      swap(u, v);
  return edges;
}

// random connected graph: start with a tree, add `extra` random edges
vector<pair<int, int>> rand_connected_graph(int n, int extra) {
  auto edges = rand_tree(n);
  set<pair<int, int>> seen(edges.begin(), edges.end());
  int attempts = 0;
  while ((int)edges.size() < n - 1 + extra && attempts < 1000000) {
    int u = randint(1, n), v = randint(1, n);
    if (u == v) {
      attempts++;
      continue;
    }
    if (u > v)
      swap(u, v);
    if (seen.count({u, v})) {
      attempts++;
      continue;
    }
    seen.insert({u, v});
    edges.push_back({u, v});
    attempts++;
  }
  shuffle(edges.begin(), edges.end(), rng);
  for (auto &[u, v] : edges)
    if (chance(1, 2))
      swap(u, v);
  return edges;
}

// random DAG on n nodes with `m` edges
vector<pair<int, int>> rand_dag(int n, int m) {
  set<pair<int, int>> seen;
  vector<pair<int, int>> edges;
  int attempts = 0;
  while ((int)edges.size() < m && attempts < 1000000) {
    int u = randint(1, n), v = randint(1, n);
    if (u >= v) {
      attempts++;
      continue;
    }
    if (seen.count({u, v})) {
      attempts++;
      continue;
    }
    seen.insert({u, v});
    edges.push_back({u, v});
    attempts++;
  }
  shuffle(edges.begin(), edges.end(), rng);
  for (auto &[u, v] : edges)
    if (chance(1, 2))
      swap(u, v);
  return edges;
}

//  PRINT HELPERS

// print a vector space-separated on one line
template <typename T> void print_array(const vector<T> &a) {
  for (int i = 0; i < (int)a.size(); i++) {
    cout << a[i];
    if (i + 1 < (int)a.size())
      cout << ' ';
  }
  cout << '\n';
}

// print edges, one per line
void print_edges(const vector<pair<int, int>> &edges) {
  for (auto [u, v] : edges)
    cout << u << ' ' << v << '\n';
}

// print a 2D grid
template <typename T> void print_grid(const vector<vector<T>> &g) {
  for (auto &row : g) {
    for (int j = 0; j < (int)row.size(); j++) {
      cout << row[j];
      if (j + 1 < (int)row.size())
        cout << ' ';
    }
    cout << '\n';
  }
}

const int MAX_T = 3;
const int MAX_N = 8;
const int MIN_A = 1;
const int MAX_A = 8;

//  [EDIT THIS]

void generate() {
  int t = randint(1, MAX_T);
  cout << t << '\n';
  while (t--) {
    // TODO: here
    int n = uniform_int_distribution<int>(1, 100)(rng);
    int m = uniform_int_distribution<int>(1, 100)(rng);
    cout << n << " " << m << "\n";
  }
}

int main(int argc, char *argv[]) {
  int s = (argc > 1) ? atoi(argv[1]) : 42;
  seed(s);
  generate();
}
