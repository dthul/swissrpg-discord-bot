FROM debian:bookworm-slim

ENV DEBIAN_FRONTEND=noninteractive
# Install AWS CLI
RUN apt-get update && \
  apt-get install -y curl groff less unzip && \
  curl "https://awscli.amazonaws.com/awscli-exe-linux-x86_64.zip" -o "awscliv2.zip" && \
  unzip awscliv2.zip && \
  rm awscliv2.zip && \
  ./aws/install && \
  rm -rf ./aws \
  apt-get remove curl unzip && \
  apt-get autoremove && \
  rm -rf /var/lib/apt/lists/*
# Install postgres client
RUN apt-get update && \
  apt-get install -y curl ca-certificates gnupg && \
  sh -c 'echo "deb http://apt.postgresql.org/pub/repos/apt bookworm-pgdg main" > /etc/apt/sources.list.d/pgdg.list' && \
  curl https://www.postgresql.org/media/keys/ACCC4CF8.asc | gpg --dearmor | tee /etc/apt/trusted.gpg.d/apt.postgresql.org.gpg >/dev/null && \
  apt-get update && \
  apt-get install -y postgresql-client-14 && \
  apt-get remove -y curl ca-certificates gnupg && \
  apt-get autoremove -y && \
  rm -rf /var/lib/apt/lists/*
COPY backup.sh /usr/local/bin/swissrpg-backup
CMD ["/bin/bash", "/usr/local/bin/swissrpg-backup"]
