# Browser phase timings (ms; median / nearest-rank p95; N=20 each)

| Workload | CPU prepare | Submit → mapped | PNG encode | Native total | Host roundtrip | HTTP body received | Decode | Request → two RAF |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| small | 0.90 / 1.12 | 2.19 / 3.34 | 7.24 / 7.69 | 10.74 / 12.42 | 11.54 / 13.25 | 16.85 / 18.70 | 0.40 / 0.60 | 47.10 / 48.10 |
| moderate | 0.88 / 1.05 | 2.25 / 3.87 | 16.37 / 18.76 | 20.64 / 22.32 | 22.13 / 23.77 | 27.70 / 31.20 | 0.30 / 0.40 | 57.40 / 59.30 |
| moderate-textured | 0.83 / 0.90 | 2.06 / 4.00 | 18.80 / 25.10 | 22.25 / 28.30 | 24.68 / 30.78 | 31.70 / 38.30 | 0.30 / 0.40 | 56.85 / 58.50 |
