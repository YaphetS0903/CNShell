# Production deployment images

The production Compose file pulls these images by immutable multi-platform manifest digest. They
are deployment dependencies and are not embedded in the CNshell desktop application.

| Component | Version | Manifest digest | License | Source |
| --- | --- | --- | --- | --- |
| NGINX unprivileged image | 1.28.0-alpine | `sha256:c97ff0bf7cbae369953c6da1232ec14ad9f971d66360c5698db0856a4cd657a0` | NGINX BSD-2-Clause; image tooling Apache-2.0 | <https://github.com/nginx/docker-nginx-unprivileged> |
| NGINX Prometheus exporter | 1.4.2 | `sha256:6edfb73afd11f2d83ea4e8007f5068c3ffaa38078a6b0ad1339e5bd2f637aacd` | Apache-2.0 | <https://github.com/nginx/nginx-prometheus-exporter> |
| Prometheus | 3.5.0 | `sha256:63805ebb8d2b3920190daf1cb14a60871b16fd38bed42b857a3182bc621f4996` | Apache-2.0 | <https://github.com/prometheus/prometheus> |
| Alertmanager | 0.28.1 | `sha256:27c475db5fb156cab31d5c18a4251ac7ed567746a2483ff264516437a39b15ba` | Apache-2.0 | <https://github.com/prometheus/alertmanager> |

The digests were resolved from the authenticated Docker Registry v2 API for the exact tags above.
Changing a version or digest requires reviewing the upstream release and license again, then
running `npm run test:relay-production-config` on Linux Docker/Compose before deployment.
