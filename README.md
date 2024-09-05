# Uptime Kuma metrics proxy

Prometheus metrics filter proxy for uptime kuma

This project is a simple proxy for https://github.com/louislam/uptime-kuma

If you are using single uptime-kuma instance for multiple projects, but want to integrate it with different prometheus
and grafana dashboards, this project is for you.

## How it works

- parses all services and specified tags from uptime-kuma (and updates it, as specified in env
  var `METRICS_PROXY_TAGS_TTL_SECONDS`)
    - there is only 1 vanilla way to get it, via socket.io connection, so we using it
- proxies all requests on uptime-kuma instance
    - if no tags specified, we acting like a direct proxy to kuma
    - if there any tag specified in url, like `/my-tag`, it will keeps only metrics, for services, labeled with
      specified tag

By that, we can add tags onto targer services in uptime-kuma and configure different prometheus collectors on:

- `https://proxy-url`

## Add to your uptime-kuma

To use it, you need follow this steps:

- create .env file with your setup
- run proxy (from binary or using docker)
- setup proxy behind reverse proxy
- (?) configure your prometheus config

### Env file


### Using docker

## Task list

- [ ] tocdown
- [ ] build binary in github actions
- [ ] tracking setup
- [ ] release docker image
- [ ] env specification tutorial
- [ ] fast start tutorial (docker and docker-compose)