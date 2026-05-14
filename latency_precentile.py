import csv

import matplotlib.pyplot as plt

xs = []
ys = []
with open("latency_percentiles.csv", "r") as f:
    reader = csv.DictReader(f)
    for row in reader:
        xs.append(float(row["quantile"]))
        ys.append(float(row["us"]))

plt.plot(xs, ys, marker="o")
plt.xlabel("Quantile")
plt.ylabel("Latency (us)")
plt.title("LRU Concurrent Latency")
plt.grid(True)
plt.savefig("latency_cdf.png", dpi=150)
print("Saved latency_cdf.png")
