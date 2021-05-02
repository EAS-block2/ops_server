# ops_server
The central coordination server for the Emergency Alert System

## Running
This is the most critical component of the system, if it goes down or is unable to communicate, the system is inoperable (see failure modes). For this reason, this program should be run in a fault tolerant enviornemnt such as
- A linux (we use Fedora server) VM on a Hyper-V / Proxmox VE / Vsphere / etc **failover** cluster 
- A Docker image on a Kubernetes / micro k8s / k3s cluster

### Installation on a VM
This assumes that you aleady have a GNU+Linux VM with systemd as the init system


## Failure Modes
- Scenario: a button thread has crashed
  - Button units will attempt to communicate with the primary port and will fall back to any other ports as defined in the `general_ports` section of ops_config.yaml
  - If all threads defined in this config are dead, button alarm activation will not work, however overrides from the web interface will continue to work
