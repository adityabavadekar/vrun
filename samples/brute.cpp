/**
 * brute.cpp — Stress test brute force template
 *
 * Keep this dead simple and obviously correct.
 * No clever optimizations — correctness only.
 *
 * Edit the constraints block and solve() at the bottom.
 */

#include <bits/stdc++.h>
#include <cstdint>
using namespace std;
#define fastio()                                                               \
  ios_base::sync_with_stdio(false);                                            \
  cin.tie(NULL);                                                               \
  cout.tie(NULL)
#define endl "\n"

using ll = long long;
using pii = pair<int, int>;
using pll = pair<ll, ll>;
using vi = vector<int>;
using vl = vector<ll>;
using vvi = vector<vi>;
using vvl = vector<vl>;

// try all subsets of [0, n)
// usage: for_each_subset(n, [&](vi& subset) { ... });
template <typename F> void for_each_subset(int n, F callback) {
  for (int mask = 0; mask < (1 << n); mask++) {
    vi subset;
    for (int i = 0; i < n; i++)
      if (mask >> i & 1)
        subset.push_back(i);
    callback(subset);
  }
}

// try all permutations of a vector
// usage: for_each_perm(v, [&](vi& p) { ... });
template <typename T, typename F> void for_each_perm(vector<T> v, F callback) {
  sort(v.begin(), v.end());
  do {
    callback(v);
  } while (next_permutation(v.begin(), v.end()));
}

// all pairs (i, j) with i < j in [0, n)
template <typename F> void for_each_pair(int n, F callback) {
  for (int i = 0; i < n; i++)
    for (int j = i + 1; j < n; j++)
      callback(i, j);
}

//  GRAPH HELPERS

// BFS distance from src; dist[v] = -1 if unreachable
vi bfs_dist(const vvi &adj, int src) {
  int n = adj.size();
  vi dist(n, -1);
  queue<int> q;
  dist[src] = 0;
  q.push(src);
  while (!q.empty()) {
    int u = q.front();
    q.pop();
    for (int v : adj[u])
      if (dist[v] == -1) {
        dist[v] = dist[u] + 1;
        q.push(v);
      }
  }
  return dist;
}

// check if undirected graph is connected
bool is_connected(const vvi &adj) {
  int n = adj.size();
  if (n == 0)
    return true;
  auto dist = bfs_dist(adj, 0);
  for (int d : dist)
    if (d == -1)
      return false;
  return true;
}

//  MATH HELPERS

ll gcd(ll a, ll b) { return b ? gcd(b, a % b) : a; }
ll lcm(ll a, ll b) { return a / gcd(a, b) * b; }

bool is_prime(ll n) {
  if (n < 2)
    return false;
  for (ll i = 2; i * i <= n; i++)
    if (n % i == 0)
      return false;
  return true;
}

// prefix sum
vi prefix(const vi &a) {
  int n = a.size();
  vi p(n + 1, 0);
  for (int i = 0; i < n; i++)
    p[i + 1] = p[i] + a[i];
  return p;
}

// sum of a[l..r] inclusive using prefix sum
int range_sum(const vi &p, int l, int r) { return p[r + 1] - p[l]; }

//  TODO: EDIT THIS

int simulate(int start, vector<int> dishes) {
  int n = dishes.size();
  int last_eater = -1;
  int cur = start;

  while (true) {
    // check if anyone has food left
    bool any = false;
    for (int x : dishes)
      if (x > 0) {
        any = true;
        break;
      }
    if (!any)
      break;

    if (dishes[cur] > 0) {
      dishes[cur]--;
      last_eater = cur;
    }
    cur = (cur + 1) % n;
  }

  return last_eater;
}

bool can_produce(const vi &a, const vi &kept_indices) {
  int n = a.size();
  int m = kept_indices.size();
  if (m == 0)
    return n == 0;
  set<int> kept_set(kept_indices.begin(), kept_indices.end());

  multiset<int> sources;

  int ki = 0;

  for (int i = 0; i < n; i++) {
    if (kept_set.count(i)) {
      sources.insert(a[i]);
    } else {
      auto it = sources.find(a[i] - 1);
      if (it == sources.end())
        return false;
      sources.erase(it);
      sources.insert(a[i]);
    }
  }
  return true;
}

//  CONSTRAINTS

const int MAX_T = 5000;
const int MAX_N = 10;
const int MAX_A = 10;
const int MIN_A = 1;

void solve() {
  int n, m;
  cin >> n >> m;
  cout << n + m << "\n";
}

int32_t main() {
  fastio();
  int t = 1;
  cin >> t; // TODO: remove if not requied
  while (t--)
    solve();
  return 0;
}
