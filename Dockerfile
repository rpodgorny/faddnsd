FROM ubuntu:focal
ENV DEBIAN_FRONTEND noninteractive
RUN apt-get update && apt-get install -y python3 python3-pip bind9 dnsutils && apt-get clean && rm -rf /var/lib/apt/lists/
WORKDIR /usr/src/app
COPY requirements.txt .
RUN pip install --no-cache-dir -i http://172.17.0.1:3141/root/pypi/+simple/ --trusted-host 172.17.0.1 -r requirements.txt
COPY . .
#ENTRYPOINT ["docker-entrypoint.sh"]
CMD ["python3", "./faddnsd.py"]
