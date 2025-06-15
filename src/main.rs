use serialport::{SerialPortInfo, SerialPortType};
use std::time::Duration;
use std::io::{BufRead, BufReader};
use std::path::Path;
use config::{Config, File};
use serde::Deserialize;
use tokio_postgres::NoTls;
use mysql_async::prelude::*;
use chrono::{DateTime, Utc};
use std::sync::{Arc, Mutex};
use tokio::time;

// ============================================================================
// Configuration structures
// ============================================================================

#[derive(Debug, Deserialize)]
#[serde(rename_all = "lowercase")]
enum DatabaseType 
{
    Postgres,
    MySQL,
    MariaDB,
}

#[derive(Debug, Deserialize)]
struct SerialConfig 
{
    port: String,
    baud_rate: u32,
    timeout_ms: u64,
}

#[derive(Debug, Deserialize)]
struct DatabaseConfig 
{
    db_type: DatabaseType,
    host: String,
    port: u16,
    user: String,
    password: String,
    db_name: String,
    table: String,
}

#[derive(Debug, Deserialize)]
struct UploadConfig 
{
    frequency: u64,
}

#[derive(Debug, Deserialize)]
struct Settings 
{
    serial: SerialConfig,
    database: DatabaseConfig,
    upload: UploadConfig,
}

// ============================================================================
// Database abstraction
// ============================================================================

enum DatabaseInner 
{
    Postgres(tokio_postgres::Client),
    MySQL(mysql_async::Pool),
}

struct Database 
{
    inner: DatabaseInner,
    table_name: String,
}

impl Database 
{
    /// Create a new database connection
    async fn new(config: &DatabaseConfig) -> Result<Self, Box<dyn std::error::Error>> 
    {
        let inner = match config.db_type 
        {
            DatabaseType::Postgres => Self::connect_postgres(config).await?,
            DatabaseType::MySQL | DatabaseType::MariaDB => Self::connect_mysql(config).await?,
        };

        let db = Database 
        {
            inner,
            table_name: config.table.clone(),
        };

        db.create_table_if_not_exists().await?;
        Ok(db)
    }

    /// PostgreSQL connection
    async fn connect_postgres(config: &DatabaseConfig) -> Result<DatabaseInner, Box<dyn std::error::Error>> 
    {
        let connection_string = format!(
            "host={} port={} user={} password={} dbname={}",
            config.host, config.port, config.user, config.password, config.db_name
        );

        let (client, connection) = tokio_postgres::connect(&connection_string, NoTls).await?;

        // Handle connection in background
        tokio::spawn(async move 
        {
            if let Err(e) = connection.await 
            {
                eprintln!("PostgreSQL connection error: {}", e);
            }
        });

        Ok(DatabaseInner::Postgres(client))
    }

    /// MySQL/MariaDB connection
    async fn connect_mysql(config: &DatabaseConfig) -> Result<DatabaseInner, Box<dyn std::error::Error>> 
    {
        let url = format!(
            "mysql://{}:{}@{}:{}/{}",
            config.user, config.password, config.host, config.port, config.db_name
        );
        
        let pool = mysql_async::Pool::new(url.as_str());
        Ok(DatabaseInner::MySQL(pool))
    }

    /// Create table if it doesn't exist
    async fn create_table_if_not_exists(&self) -> Result<(), Box<dyn std::error::Error>> 
    {
        match &self.inner 
        {
            DatabaseInner::Postgres(client) => 
            {
                let query = format!(
                    "CREATE TABLE IF NOT EXISTS {} (
                        id SERIAL PRIMARY KEY,
                        timestamp TIMESTAMP WITH TIME ZONE NOT NULL,
                        value TEXT NOT NULL
                    )",
                    self.table_name
                );
                client.execute(&query, &[]).await?;
            }
            DatabaseInner::MySQL(pool) => 
            {
                let query = format!(
                    "CREATE TABLE IF NOT EXISTS {} (
                        id INT AUTO_INCREMENT PRIMARY KEY,
                        timestamp TIMESTAMP NOT NULL,
                        value TEXT NOT NULL
                    )",
                    self.table_name
                );
                let mut conn = pool.get_conn().await?;
                conn.query_drop(query).await?;
            }
        }
        Ok(())
    }

    /// Insert a value into the database
    async fn insert_value(&self, value: &str) -> Result<(), Box<dyn std::error::Error>> 
    {
        let now: DateTime<Utc> = Utc::now();
        
        match &self.inner 
        {
            DatabaseInner::Postgres(client) => 
            {
                let query = format!(
                    "INSERT INTO {} (timestamp, value) VALUES ($1, $2)",
                    self.table_name
                );
                client.execute(&query, &[&now, &value]).await?;
            }
            DatabaseInner::MySQL(pool) => 
            {
                let query = format!(
                    "INSERT INTO {} (timestamp, value) VALUES (?, ?)",
                    self.table_name
                );
                let mut conn = pool.get_conn().await?;
                let mysql_timestamp = now.format("%Y-%m-%d %H:%M:%S").to_string();
                conn.exec_drop(query, (mysql_timestamp, value)).await?;
            }
        }
        Ok(())
    }
}

// ============================================================================
// Serial port utilities
// ============================================================================

struct SerialPortManager;

impl SerialPortManager 
{
    /// List all available serial ports
    fn list_available_ports() -> Result<Vec<SerialPortInfo>, Box<dyn std::error::Error>> 
    {
        let ports = serialport::available_ports()?;
        Ok(ports)
    }

    /// Display available serial ports
    fn display_available_ports() 
    {
        println!("Available serial ports:");
        
        match Self::list_available_ports() 
        {
            Ok(ports) => 
            {
                for port in ports 
                {
                    match port.port_type 
                    {
                        SerialPortType::UsbPort(info) => 
                        {
                            println!("  USB - {} ({})", 
                                port.port_name, 
                                info.product.unwrap_or_default()
                            );
                        }
                        SerialPortType::PciPort => 
                        {
                            println!("  PCI - {}", port.port_name);
                        }
                        SerialPortType::BluetoothPort => 
                        {
                            println!("  Bluetooth - {}", port.port_name);
                        }
                        SerialPortType::Unknown => 
                        {
                            println!("  Unknown - {}", port.port_name);
                        }
                    }
                }
            }
            Err(e) => 
            {
                eprintln!("Error while searching for serial ports: {}", e);
            }
        }
    }

    /// Open a serial port with the given configuration
    fn open_port(config: &SerialConfig) -> Result<Box<dyn serialport::SerialPort>, Box<dyn std::error::Error>> 
    {
        let port = serialport::new(&config.port, config.baud_rate)
            .timeout(Duration::from_millis(config.timeout_ms))
            .open()?;
        
        Ok(port)
    }
}

// ============================================================================
// Configuration management
// ============================================================================

struct ConfigManager;

impl ConfigManager 
{
    /// Load configuration from TOML file
    fn load() -> Result<Settings, Box<dyn std::error::Error>> 
    {
        let config_path = Path::new("config/default.toml");
        
        let settings = Config::builder()
            .add_source(File::from(config_path))
            .build()?;

        let settings = settings.try_deserialize()?;
        Ok(settings)
    }

    /// Display current configuration
    fn display(settings: &Settings) 
    {
        println!("\nCurrent configuration:");
        println!("  Port: {}", settings.serial.port);
        println!("  Baud rate: {}", settings.serial.baud_rate);
        println!("  Timeout: {} ms", settings.serial.timeout_ms);
        println!("  Upload frequency: {} seconds", settings.upload.frequency);
        println!("  Database: {:?}", settings.database.db_type);
        println!("  Table: {}", settings.database.table);
    }
}

// ============================================================================
// Data processor
// ============================================================================

struct DataProcessor 
{
    last_value: Arc<Mutex<Option<String>>>,
}

impl DataProcessor 
{
    fn new() -> Self 
    {
        Self 
        {
            last_value: Arc::new(Mutex::new(None)),
        }
    }

    /// Process a new received line
    fn process_line(&self, line: &str) 
    {
        let trimmed_line = line.trim();
        
        if !trimmed_line.is_empty() 
        {
            println!("Received line: {}", trimmed_line);
            
            // Update last value only if it has changed
            let mut last = self.last_value.lock().unwrap();
            if last.as_ref() != Some(&trimmed_line.to_string()) 
            {
                *last = Some(trimmed_line.to_string());
            }
        }
    }

    /// Start periodic upload task
    async fn start_upload_task(&self, database: Database, upload_frequency: u64) 
    {
        let last_value_clone = Arc::clone(&self.last_value);
        
        tokio::spawn(async move 
        {
            let mut interval = time::interval(Duration::from_secs(upload_frequency));
            
            loop 
            {
                interval.tick().await;
                
                let value = {
                    let mut guard = last_value_clone.lock().unwrap();
                    guard.take()
                };
                
                if let Some(val) = value 
                {
                    match database.insert_value(&val).await 
                    {
                        Ok(_) => 
                        {
                            println!("✓ Value successfully uploaded: {}", val);
                        }
                        Err(e) => 
                        {
                            eprintln!("✗ Upload error: {}", e);
                        }
                    }
                }
            }
        });
    }
}

// ============================================================================
// Serial reader
// ============================================================================

struct SerialReader;

impl SerialReader 
{
    /// Read data from serial port continuously
    async fn read_continuous(
        port: Box<dyn serialport::SerialPort>, 
        processor: &DataProcessor
    ) -> Result<(), Box<dyn std::error::Error>> 
    {
        let reader = BufReader::new(port);
        let mut lines = reader.lines();
        
        println!("\n🔄 Reading data from serial port...");
        
        loop 
        {
            match lines.next() 
            {
                Some(Ok(line)) => 
                {
                    processor.process_line(&line);
                }
                Some(Err(e)) => 
                {
                    if e.kind() == std::io::ErrorKind::TimedOut 
                    {
                        // In case of timeout, continue
                        continue;
                    }
                    eprintln!("Error while reading serial port: {}", e);
                    break;
                }
                None => 
                {
                    // Wait a bit if no line is available
                    tokio::time::sleep(Duration::from_millis(100)).await;
                    continue;
                }
            }
        }
        
        Ok(())
    }
}

// ============================================================================
// Application main
// ============================================================================

struct Application;

impl Application 
{
    /// Main application entry point
    async fn run() -> Result<(), Box<dyn std::error::Error>> 
    {
        println!("🚀 Starting serial reading application");
        
        // Load configuration
        let settings = ConfigManager::load()?;
        
        // Display system information
        SerialPortManager::display_available_ports();
        ConfigManager::display(&settings);
        
        // Open serial port
        let port = SerialPortManager::open_port(&settings.serial)?;
        println!("\n✓ Serial port successfully opened");
        
        // Initialize database connection
        let database = Database::new(&settings.database).await?;
        println!("✓ Database connection established");
        
        // Initialize data processor
        let processor = DataProcessor::new();
        
        // Start upload task
        processor.start_upload_task(database, settings.upload.frequency).await;
        println!("✓ Upload task started");
        
        // Start serial reading
        SerialReader::read_continuous(port, &processor).await?;
        
        Ok(())
    }
}

// ============================================================================
// Main function
// ============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> 
{
    if let Err(e) = Application::run().await 
    {
        eprintln!("❌ Fatal error: {}", e);
        println!("\nPress Enter to exit...");
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        std::process::exit(1);
    }
    
    println!("\nPress Enter to exit...");
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    
    Ok(())
}