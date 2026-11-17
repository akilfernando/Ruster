# Matched pipeline timing (ms, median / nearest-rank p95)

| Workload | Native render-only | Native PNG | Browser native | Browser body received | Browser decode + 2 RAF | Edit commit | Edit request → RAF |
| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: |
| small | 3.85 / 5.03 | 12.07 / 14.44 | 11.09 / 13.76 | 17.50 / 20.00 | 48.80 / 50.30 | 0.12 / 0.27 | 44.80 / 46.20 |
| moderate | 2.60 / 4.20 | 12.48 / 20.04 | 12.17 / 24.56 | 17.45 / 31.50 | 46.75 / 63.70 | 0.34 / 0.79 | 59.10 / 60.50 |
| moderate-textured | 3.48 / 4.82 | 22.15 / 24.04 | 21.09 / 25.66 | 30.15 / 37.80 | 51.45 / 67.90 | 1.07 / 2.61 | 61.90 / 64.50 |
