# acme-distributor

Service that hands out certificates to machines requesting them

This allows you to aquire all certificates through one central server, keeping your DNS credentials safe, while giving you full flexibility over the certificates being used.

What it can do:
- Aquire Let's Encrypt certificates using DNS Challenge and distribute them
- Distribute custom, uploaded certificates
- Be extended into supporting whatever CA and certificate aquisition API you wish to have supported

