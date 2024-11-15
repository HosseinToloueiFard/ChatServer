use std::collections::HashMap;
use std::io::{self, Write, Read, BufReader, BufRead};
use std::net::{TcpListener, TcpStream};
use std::sync::{Arc, Mutex};
use std::thread;
use std::fs::{File, OpenOptions};
use serde::{Serialize, Deserialize};
use serde_json;

#[derive(Serialize, Deserialize, Debug)]
struct Credentials {
    users: HashMap<String, String>,
}

fn load_credentials() -> Credentials {
    let file = OpenOptions::new().read(true).open("credentials.json");
    match file {
        Ok(file) => serde_json::from_reader(file).unwrap_or_else(|_| Credentials {
            users: HashMap::new(),
        }),
        Err(_) => Credentials { users: HashMap::new() },
    }
}

fn save_credentials(credentials: &Credentials) {
    let file = OpenOptions::new().write(true).create(true).open("credentials.json").unwrap();
    serde_json::to_writer(file, &credentials).unwrap();
}

fn handle_client(mut stream: TcpStream, clients: Arc<Mutex<HashMap<String, TcpStream>>>, username: String) {
    let mut buffer = [0; 512]; 

    loop {
        match stream.read(&mut buffer) {
            Ok(0) => {
                println!("Client {} disconnected", username);
                break;
            }
            Ok(bytes_read) => {
                let message = String::from_utf8_lossy(&buffer[..bytes_read]).to_string();
                if !message.trim().is_empty() {
                    println!("Received message from {}: {}", username, message);

                    let clients = clients.lock().unwrap(); 
                    for (client_username, mut client_stream) in clients.iter() {
                        if client_username != &username {
                            let message_to_send = format!("{}: {}\n", username, message);
                            if let Err(e) = client_stream.write_all(message_to_send.as_bytes()) {
                                eprintln!("Failed to send message to {}: {}", client_username, e);
                            }
                        }
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to read from stream: {}", e);
                break;
            }
        }
    }

    let mut clients = clients.lock().unwrap();
    clients.remove(&username);
}

fn main() {
    ctrlc::set_handler(move || {
        println!("Server shutting down...");
        std::process::exit(0);
    }).expect("Error setting Ctrl-C handler");

    let listener = TcpListener::bind("127.0.0.1:8080").unwrap();
    println!("Server listening on 127.0.0.1:8080");

    let clients: Arc<Mutex<HashMap<String, TcpStream>>> = Arc::new(Mutex::new(HashMap::new()));
    let credentials = Arc::new(Mutex::new(load_credentials()));

    loop {
        match listener.accept() {
            Ok((mut stream, _)) => {
                let mut username = String::new();
                let mut password = String::new();
                let mut reader = BufReader::new(&stream);

                if reader.read_line(&mut username).is_ok() {
                    let username = username.trim().to_string();
                    println!("New client connected: {}", username);

                    if reader.read_line(&mut password).is_ok() {
                        let password = password.trim().to_string();

                        let mut credentials = credentials.lock().unwrap();

                        if let Some(stored_password) = credentials.users.get(&username) {
                            if stored_password == &password {
                                println!("Welcome back, {}!", username);
                                stream.write_all(b"Login successful!\n").unwrap();
                            } else {
                                println!("Incorrect password. Please try again.");
                                stream.write_all(b"Incorrect password.\n").unwrap();
                                continue;
                            }
                        } else {
                            credentials.users.insert(username.clone(), password);
                            save_credentials(&credentials);
                            println!("New user {} registered successfully!", username);
                            stream.write_all(b"New user registered successfully!\n").unwrap();
                        }

                        let clients = Arc::clone(&clients);
                        {
                            let mut clients_lock = clients.lock().unwrap();
                            clients_lock.insert(username.clone(), stream.try_clone().unwrap());
                        }

                        let clients_clone = Arc::clone(&clients);
                        thread::spawn({
                            let clients = Arc::clone(&clients_clone);
                            move || {
                                handle_client(stream, clients, username);
                            }
                        });
                    }
                }
            }
            Err(e) => {
                eprintln!("Failed to accept connection: {}", e);
            }
        }
    }
}
