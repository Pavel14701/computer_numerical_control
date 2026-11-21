# eid — headless LinuxCNC sim (REAL interpreter rs274, no GUI)
FROM debian:bookworm-slim

RUN apt-get update && \
    apt-get install -y --no-install-recommends \
        linuxcnc-uspace \
    && apt-get clean && rm -rf /var/lib/apt/lists/*

# Минимальная tool table для headless rs274
RUN mkdir -p /usr/share/doc/linuxcnc/examples/sample-configs/common && \
    printf "T1 P0 X0.0000 Y0.0000 Z0.0000\n" > /usr/share/doc/linuxcnc/examples/sample-configs/common/tool.tbl && \
    printf "" > /usr/share/doc/linuxcnc/examples/sample-configs/common/rs274.var

WORKDIR /work
