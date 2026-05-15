# sandbox/scanner:latest — ephemeral ClamAV scan container.
#
# Used by `sandbox scan --with-clamav` (one-shot scan) and
# `sandbox scan --update-db` (freshclam refresh). Per ADR-0008 the AV layer
# runs inside this container, never on the host and never inside the
# project's language container.
#
# Signature database lives in a separate named volume (`sandbox-scanner-db`)
# mounted at /var/lib/clamav so the ~300 MB DB persists across scans and the
# image itself stays small.
#
# Image is built locally by `sandbox-docker::scanner::ensure_image` the first
# time the user opts into ClamAV (no public registry push for v0.1).

FROM alpine:3.20

RUN apk add --no-cache \
        clamav \
        clamav-libunrar \
        clamav-daemon \
        ca-certificates \
    && rm -rf /var/cache/apk/*

# Pre-create the signature dir with the right ownership so freshclam can
# write without --user gymnastics.
RUN mkdir -p /var/lib/clamav /var/log/clamav \
    && chown -R clamav:clamav /var/lib/clamav /var/log/clamav \
    && chmod 750 /var/lib/clamav

# Minimal freshclam config: log to stdout (so `docker logs` captures the
# update), no daemon mode, no notify, single mirror retry.
RUN printf '%s\n' \
        'DatabaseDirectory /var/lib/clamav' \
        'UpdateLogFile /dev/stdout' \
        'LogVerbose yes' \
        'LogTime yes' \
        'NotifyClamd no' \
        'DatabaseMirror database.clamav.net' \
        'MaxAttempts 3' \
    > /etc/clamav/freshclam.conf

# Minimal clamscan config: bytecode and HeuristicAlerts on, follow-symlinks
# off so we don't escape the bind mount.
RUN printf '%s\n' \
        'DatabaseDirectory /var/lib/clamav' \
        'LogVerbose no' \
        'LogTime yes' \
        'FollowDirectorySymlinks no' \
        'FollowFileSymlinks no' \
        'HeuristicAlerts yes' \
    > /etc/clamav/clamd.conf

USER clamav
WORKDIR /scan

# Default entrypoint is `clamscan`; --update-db overrides via `--entrypoint
# freshclam`. We avoid CMD here so the caller fully controls the invocation.
ENTRYPOINT ["clamscan"]
