FROM alpine:3.21

ARG TARGETPLATFORM

WORKDIR /opt/junction
RUN adduser -D junction --uid 7749 && chown -R junction:junction /opt/junction

COPY ./${TARGETPLATFORM}/junction /usr/bin/junction
COPY ./${TARGETPLATFORM}/junction-merger /usr/bin/junction-merger

USER junction
ENTRYPOINT [ "/usr/bin/junction" ]
