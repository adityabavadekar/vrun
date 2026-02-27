// correct solution, should pass every test
#include <bits/stdc++.h>
using namespace std;
#define fastio()                                                               \
  ios_base::sync_with_stdio(false);                                            \
  cin.tie(NULL);                                                               \
  cout.tie(NULL)
#define endl "\n"

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
