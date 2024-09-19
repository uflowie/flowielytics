use sysinfo::System;

pub struct ProcessInfoProvider {
    sys: System,
}

impl ProcessInfoProvider {
    pub fn new() -> ProcessInfoProvider {
        Self {
            sys: System::new_all()
        }
    }
    
    pub async fn get_token_and_port(&mut self) -> (String, String) {
        loop {
            self.sys.refresh_all();
    
            let mut token = None;
            let mut port = None;
    
            for process in self.sys.processes_by_name("LeagueClientUx.exe") {
                for arg in process.cmd() {
                    if let Some(t) = arg.strip_prefix("--remoting-auth-token=") {
                        token = Some(t.to_string());
                    }
                    if let Some(p) = arg.strip_prefix("--app-port=") {
                        port = Some(p.to_string());
                    }
                }
            }
    
            if let (Some(token), Some(port)) = (token, port) {
                println!("port: {}", port);
                return (token, port);
            }
    
            println!("Couldn't find auth token, sleeping for 1 second");
            tokio::time::sleep(std::time::Duration::from_secs(1)).await;
        }
    }
}

