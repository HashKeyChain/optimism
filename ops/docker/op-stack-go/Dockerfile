# automatically set by buildkit, can be changed with --platform flag
# see https://docs.docker.com/engine/reference/builder/#automatic-platform-args-in-the-global-scope
# TARGETOS
# TARGETARCH
# TARGETPLATFORM
# BUILDPLATFORM

ARG BUILDPLATFORM="linux/amd64"

# All target images use this as base image, and add the final build results.
# It will default to the target platform.
ARG TARGET_BASE_IMAGE=alpine:3.20

# The ubuntu target base image is used for the op-challenger build with kona and asterisc.
ARG UBUNTU_TARGET_BASE_IMAGE=ubuntu:22.04

# The version of kona to use.
# The only build that uses this is `op-challenger-target`.
ARG KONA_VERSION=none

# The version of asterisc to use.
# The only build that uses this is `op-challenger-target`.
ARG ASTERISC_VERSION=none

# builder_foundry does not need to be built on $BUILDPLATFORM, as foundry produces static binaries.
# FROM alpine:3.21 AS builder_foundry

# # Build args are only-available in top-level by default, so we'll need to declare it inside the stage too
# ARG TARGETARCH

# RUN apk add --no-cache curl bash git jq

# COPY ./op-deployer/pkg/deployer/forge/version.json /tmp/op-deployer-versions.json

# # Verify forge checksums
# RUN <<-EOF
#     #!/bin/bash
#     set -e

#     FOUNDRY_VERSION="$(jq -r '.forge' /tmp/op-deployer-versions.json)"
#     FOUNDRY_PLATFORM=alpine_${TARGETARCH}
#     FOUNDRY_URL="https://github.com/foundry-rs/foundry/releases/download/${FOUNDRY_VERSION}/foundry_${FOUNDRY_VERSION}_${FOUNDRY_PLATFORM}.tar.gz"
#     DOWNLOAD_PATH="/tmp/forge.tar.gz"

#     # Download the artifact archive
#     curl -fL -o ${DOWNLOAD_PATH} "${FOUNDRY_URL}"

#     # Ensure the checksum matches our expectations
#     ACTUAL_SHA="$(sha256sum "${DOWNLOAD_PATH}" | awk '{print $1}')"
#     EXPECTED_SHA="$(jq -r --arg arch "${FOUNDRY_PLATFORM}" '.checksums[$arch]' /tmp/op-deployer-versions.json)"

#     [ "${ACTUAL_SHA}" == "${EXPECTED_SHA}" ] || (echo "Forge checksum mismatch: Expected $EXPECTED_SHA, got $ACTUAL_SHA" && exit 1)

#     # Extract the archive
#     tar -xzf "${DOWNLOAD_PATH}" -C /usr/local/bin forge
# EOF

# Display installed version for visibility
# RUN forge --version

# We may be cross-building for another platform. Specify which platform we need as builder.
FROM  golang:1.24.10-alpine3.21 AS builder

RUN apk add --no-cache curl tar gzip make gcc musl-dev linux-headers git jq yq-go bash just

ARG TARGETARCH

# We copy the go.mod/sum first, so the `go mod download` does not have to re-run if dependencies do not change.
COPY ./go.mod /app/go.mod
COPY ./go.sum /app/go.sum

WORKDIR /app

RUN echo "go mod cache: $(go env GOMODCACHE)"
RUN echo "go build cache: $(go env GOCACHE)"

# warm-up the cache
RUN go mod download

# NOTE: the Dockerfile.dockerignore file effectively describes all dependencies
COPY . /app

# We avoid copying the full .git dir into the build for just some metadata.
# Instead, specify:
# --build-arg GIT_COMMIT=$(git rev-parse HEAD)
# --build-arg GIT_DATE=$(git show -s --format='%ct')
ARG GIT_COMMIT
ARG GIT_DATE

ARG TARGETOS

# Show just version
RUN just --version

# separate docker-builds:
# - proxyd
# - any JS/TS/smart-contract builds

# Build the Go services, utilizing caches and share the many common packages.
# The "id" defaults to the value of "target", the cache will thus be reused during this build.
# "sharing" defaults to "shared", the cache will thus be available to other concurrent docker builds.

# 只构建当前版本 cannon（去掉需要拉取外部镜像的 legacy embeds）。
# 当前 make cannon 会把 cannon64-impl 嵌入 multicannon/embeds/cannon-8，支持当前 StateVersion。
# 如需兼容旧版本游戏，再补 COPY --from=cannon:vX.Y.Z 的 legacy embeds。
FROM --platform=$BUILDPLATFORM builder AS cannon-builder
ARG CANNON_VERSION=v0.0.0
# ARG 跨 stage 会失效，必须在本 stage 重新声明，否则 GIT_COMMIT/GIT_DATE/TARGET* 为空。
ARG GIT_COMMIT
ARG GIT_DATE
ARG TARGETOS
ARG TARGETARCH
RUN cd cannon && make cannon  \
  GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE VERSION="$CANNON_VERSION"

FROM --platform=$BUILDPLATFORM builder AS op-program-builder
ARG OP_PROGRAM_VERSION=v0.0.0
# ARG 跨 stage 会失效，必须在本 stage 重新声明，否则 GIT_COMMIT/GIT_DATE/TARGET* 为空。
ARG GIT_COMMIT
ARG GIT_DATE
ARG TARGETOS
ARG TARGETARCH
# note: we only build the host, that's all the user needs. No Go MIPS cross-build in docker
RUN cd op-program && make op-program-host  \
  GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE VERSION="$OP_PROGRAM_VERSION"

# FROM --platform=$BUILDPLATFORM builder AS op-wheel-builder
# ARG OP_WHEEL_VERSION=v0.0.0
# RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build cd op-wheel && make op-wheel  \
#   GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE  VERSION="$OP_WHEEL_VERSION"

FROM --platform=$BUILDPLATFORM builder AS op-node-builder
ARG OP_NODE_VERSION=v0.0.0
# ARG 跨 stage 会失效，必须在本 stage 重新声明，否则 GIT_COMMIT/GIT_DATE/TARGET* 为空，
# 版本信息（GitCommit/GitDate）不会被 ldflags 编进二进制。
ARG GIT_COMMIT
ARG GIT_DATE
ARG TARGETOS
ARG TARGETARCH
RUN cd op-node && make op-node  \
  GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE VERSION="$OP_NODE_VERSION"

FROM --platform=$BUILDPLATFORM builder AS op-challenger-builder
ARG OP_CHALLENGER_VERSION=v0.0.0
# ARG 跨 stage 会失效，必须在本 stage 重新声明，否则 GIT_COMMIT/GIT_DATE/TARGET* 为空。
ARG GIT_COMMIT
ARG GIT_DATE
ARG TARGETOS
ARG TARGETARCH
RUN cd op-challenger && make op-challenger  \
  GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE  VERSION="$OP_CHALLENGER_VERSION"

# FROM --platform=$BUILDPLATFORM builder AS op-dispute-mon-builder
# ARG OP_DISPUTE_MON_VERSION=v0.0.0
# RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build cd op-dispute-mon && make op-dispute-mon  \
#   GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE  VERSION="$OP_DISPUTE_MON_VERSION"

# FROM --platform=$BUILDPLATFORM builder AS op-batcher-builder
# ARG OP_BATCHER_VERSION=v0.0.0
# RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build cd op-batcher && make op-batcher  \
#   GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE  VERSION="$OP_BATCHER_VERSION"

# FROM --platform=$BUILDPLATFORM builder AS op-proposer-builder
# ARG OP_PROPOSER_VERSION=v0.0.0
# RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build cd op-proposer && make op-proposer  \
#   GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE  VERSION="$OP_PROPOSER_VERSION"

# FROM --platform=$BUILDPLATFORM builder AS op-conductor-builder
# ARG OP_CONDUCTOR_VERSION=v0.0.0
# RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build cd op-conductor && make op-conductor  \
#   GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE  VERSION="$OP_CONDUCTOR_VERSION"

# FROM --platform=$BUILDPLATFORM builder AS da-server-builder
# RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build cd op-alt-da && make da-server  \
#   GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE

# FROM --platform=$BUILDPLATFORM builder AS op-supervisor-builder
# ARG OP_SUPERVISOR_VERSION=v0.0.0
# RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build cd op-supervisor && make op-supervisor  \
#   GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE  VERSION="$OP_SUPERVISOR_VERSION"

# FROM --platform=$BUILDPLATFORM builder AS op-supernode-builder
# ARG OP_SUPERNODE_VERSION=v0.0.0
# RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build cd op-supernode && make op-supernode  \
#   GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE  VERSION="$OP_SUPERNODE_VERSION"

# FROM --platform=$BUILDPLATFORM builder AS op-test-sequencer-builder
# ARG OP_TEST_SEQUENCER_VERSION=v0.0.0
# RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build cd op-test-sequencer && make op-test-sequencer  \
#   GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE  VERSION="$OP_TEST_SEQUENCER_VERSION"

# FROM --platform=$BUILDPLATFORM builder AS op-deployer-builder
# ARG OP_DEPLOYER_VERSION=v0.0.0
# COPY --from=builder_foundry /usr/local/bin/forge /usr/local/bin/forge
# RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build just \
#   GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE VERSION="$OP_DEPLOYER_VERSION" \
#   op-deployer/build-go

# FROM --platform=$BUILDPLATFORM builder AS op-dripper-builder
# ARG OP_DRIPPER_VERSION=v0.0.0
# RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build cd op-dripper && make op-dripper  \
#   GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE VERSION="$OP_DRIPPER_VERSION"

# FROM --platform=$BUILDPLATFORM builder AS op-faucet-builder
# ARG OP_FAUCET_VERSION=v0.0.0
# RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build just \
#   GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE VERSION="$OP_FAUCET_VERSION" \
#   op-faucet/op-faucet

# FROM --platform=$BUILDPLATFORM builder AS op-interop-mon-builder
# ARG OP_INTEROP_MON_VERSION=v0.0.0
# RUN --mount=type=cache,target=/go/pkg/mod --mount=type=cache,target=/root/.cache/go-build just \
#   GOOS=$TARGETOS GOARCH=$TARGETARCH GITCOMMIT=$GIT_COMMIT GITDATE=$GIT_DATE VERSION="$OP_INTEROP_MON_VERSION" \
#   op-interop-mon/op-interop-mon

# FROM $TARGET_BASE_IMAGE AS cannon-target
# COPY --from=cannon-builder /app/cannon/bin/cannon /usr/local/bin/
# COPY --from=cannon-builder /app/cannon/multicannon/embeds/* /usr/local/bin/
# CMD ["cannon"]

# FROM $TARGET_BASE_IMAGE AS op-program-target
# COPY --from=op-program-builder /app/op-program/bin/op-program /usr/local/bin/
# CMD ["op-program"]

# FROM $TARGET_BASE_IMAGE AS op-wheel-target
# COPY --from=op-wheel-builder /app/op-wheel/bin/op-wheel /usr/local/bin/
# CMD ["op-wheel"]

FROM $TARGET_BASE_IMAGE AS op-node-target
COPY --from=op-node-builder /app/op-node/bin/op-node /usr/local/bin/
CMD ["op-node"]

# op-challenger 镜像：内置 cannon + op-program(host)，用于运行 cannon 证明系统。
# 已裁掉 kona / asterisc（无需拉取外部镜像），故用默认的 alpine TARGET_BASE_IMAGE。
# builder 基于 golang:alpine，产出的二进制链接 musl，可直接在 alpine 运行。
FROM $TARGET_BASE_IMAGE AS op-challenger-target
RUN apk add --no-cache ca-certificates
COPY --from=op-challenger-builder /app/op-challenger/bin/op-challenger /usr/local/bin/
# Copy in op-program and cannon
COPY --from=op-program-builder /app/op-program/bin/op-program /usr/local/bin/
ENV OP_CHALLENGER_CANNON_SERVER=/usr/local/bin/op-program
COPY --from=cannon-builder /app/cannon/bin/cannon /usr/local/bin/
ENV OP_CHALLENGER_CANNON_BIN=/usr/local/bin/cannon
CMD ["op-challenger"]

# FROM $TARGET_BASE_IMAGE AS op-dispute-mon-target
# COPY --from=op-dispute-mon-builder /app/op-dispute-mon/bin/op-dispute-mon /usr/local/bin/
# CMD ["op-dispute-mon"]

# FROM $TARGET_BASE_IMAGE AS op-batcher-target
# COPY --from=op-batcher-builder /app/op-batcher/bin/op-batcher /usr/local/bin/
# CMD ["op-batcher"]

# FROM $TARGET_BASE_IMAGE AS op-proposer-target
# COPY --from=op-proposer-builder /app/op-proposer/bin/op-proposer /usr/local/bin/
# CMD ["op-proposer"]

# FROM $TARGET_BASE_IMAGE AS op-conductor-target
# COPY --from=op-conductor-builder /app/op-conductor/bin/op-conductor /usr/local/bin/
# CMD ["op-conductor"]

# FROM $TARGET_BASE_IMAGE AS da-server-target
# COPY --from=da-server-builder /app/op-alt-da/bin/da-server /usr/local/bin/
# CMD ["da-server"]

# FROM $TARGET_BASE_IMAGE AS op-supervisor-target
# COPY --from=op-supervisor-builder /app/op-supervisor/bin/op-supervisor /usr/local/bin/
# CMD ["op-supervisor"]

# FROM $TARGET_BASE_IMAGE AS op-supernode-target
# COPY --from=op-supernode-builder /app/op-supernode/bin/op-supernode /usr/local/bin/
# CMD ["op-supernode"]

# FROM $TARGET_BASE_IMAGE AS op-test-sequencer-target
# COPY --from=op-test-sequencer-builder /app/op-test-sequencer/bin/op-test-sequencer /usr/local/bin/
# CMD ["op-test-sequencer"]

# FROM $TARGET_BASE_IMAGE AS op-deployer-target
# RUN apk add --no-cache ca-certificates
# COPY --from=op-deployer-builder /usr/local/bin/forge /app/op-deployer/bin/op-deployer /usr/local/bin/
# ENV FORGE_ENV=alpine
# CMD ["op-deployer"]

# FROM $TARGET_BASE_IMAGE AS op-dripper-target
# COPY --from=op-dripper-builder /app/op-dripper/bin/op-dripper /usr/local/bin/
# CMD ["op-dripper"]

# FROM $TARGET_BASE_IMAGE AS op-faucet-target
# COPY --from=op-faucet-builder /app/op-faucet/bin/op-faucet /usr/local/bin/
# CMD ["op-faucet"]

# FROM $TARGET_BASE_IMAGE AS op-interop-mon-target
# COPY --from=op-interop-mon-builder /app/op-interop-mon/bin/op-interop-mon /usr/local/bin/
# CMD ["op-interop-mon"]
