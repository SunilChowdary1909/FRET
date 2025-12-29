# Develop using a Container
Use podamn oder docker to provide the environment.
## New to Podman / Docker?
Install either [Docker](https://www.docker.com/) or [Podman](https://podman.io/get-started) (replace `docker` in the following commands with `podman`)
## Building the Container
```sh
docker build -t fretenv .
```
## Creating Container
```sh
docker create --name fretbuilder -v fretnixstore:/nix/store -v ../:/root/FRET localhost/fretenv:latest
```
The volume ``fretnixstore`` is optional. It is used to cache the packges installed using nix in case you need to re-create the container.
Additionally, you can pass the following options to access the environment over ssh:
```sh
-p 2222:22 # 2222 is the ssh port of the container
-v $SOMEWHERE:/root # somewhere with a .ssh directory
```
## Starting the Container
```sh
docker start fretbuilder
```
## Entering the Container
```sh
docker exec -it fretbuilder bash
```
## Using Nix
```sh
cd ~/FRET
nix develop # or nix-shell
```
If you want to load the nix-shell automatically:
```sh
eval "$(direnv hook bash)"
direnv allow
```
## Removing the Environment
```sh
docker stop fretbuilder
docker container rm fretbuilder
docker image rm fretenv
```
## Potential Issues
If you run into a limit on threads when using podman, use ``podman create --pids-limit=8192 ...``