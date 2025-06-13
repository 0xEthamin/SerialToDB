# Serial to Database Bridge

[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Platform](https://img.shields.io/badge/platform-Linux%20%7C%20Windows-blue)]()
[![Database](https://img.shields.io/badge/database-PostgreSQL%20%7C%20MySQL%20%7C%20MariaDB-green)]()

A robust Rust application that reads data from serial ports and stores it in databases. Designed for IoT applications, sensor data logging, and embedded system integration.

---

## Overview

This application provides a reliable bridge between serial devices and databases, featuring automatic reconnection, data validation, and support for multiple database backends. Perfect for continuous data acquisition from microcontrollers, sensors, or any serial-enabled device.

---

## Features

- **Multi-database support**: PostgreSQL, MySQL, and MariaDB
- **Robust serial communication** with configurable timeouts and error handling
- **Automatic table creation** if not exists
- **Configurable upload frequency** to prevent database overload
- **Data deduplication** to avoid storing identical consecutive values
- **Asynchronous processing** for optimal performance
- **Comprehensive error handling** and logging

---

## Hardware Compatibility

- **Serial Devices**: Arduino, ESP32, Raspberry Pi, or any device with UART output
- **Operating Systems**: Linux, Windows
- **Databases**: PostgreSQL 9.5+, MySQL 5.7+, MariaDB 10.2+

---

## Configuration

Create a configuration file at `config/default.toml`:

```toml
[serial]
# Serial port configuration
# Linux: /dev/ttyACM0, /dev/ttyUSB0
# Windows: COM1, COM2, etc.
port = "/dev/ttyACM0"
baud_rate = 9600
timeout_ms = 1000

[database]
# Database type: postgres, mysql, or mariadb
db_type = "postgres"
host = "localhost"
port = 5432
user = "your_username"
password = "your_password"
db_name = "your_database"
table = "sensor_data"

[upload]
# Upload frequency in seconds
# Recommended: 2+ seconds to avoid database overload
frequency = 2
```

### Configuration Parameters

#### Serial Configuration
| Parameter    | Description                          | Example Values              |
|--------------|--------------------------------------|-----------------------------|
| `port`       | Serial port device path             | `/dev/ttyACM0`, `COM3`      |
| `baud_rate`  | Communication speed                  | `9600`, `115200`            |
| `timeout_ms` | Read timeout in milliseconds        | `1000`                      |

#### Database Configuration
| Parameter  | Description                    | Values                          |
|------------|--------------------------------|---------------------------------|
| `db_type`  | Database type                  | `postgres`, `mysql`, `mariadb`  |
| `host`     | Database server hostname       | `localhost`, `app.garageisep.com`   |
| `port`     | Database server port           | `5432` (PostgreSQL), `3306` (MySQL) |
| `user`     | Database username              | `app_user`                      |
| `password` | Database password              | `secure_password`               |
| `db_name`  | Target database name           | `app_db`                     |
| `table`    | Target table name              | `temperature_data`              |

#### Upload Configuration
| Parameter   | Description                     | Recommended |
|-------------|---------------------------------|-------------|
| `frequency` | Upload interval in seconds      | `2` or higher |

---

## Usage

1. **Setup your serial device** to output data line by line
2. **Configure the database** and ensure connectivity
4. **Run the application**:


---

## Database Schema

The application automatically creates a table with the following structure:

### PostgreSQL
```sql
CREATE TABLE IF NOT EXISTS your_table_name (
    id SERIAL PRIMARY KEY,
    timestamp TIMESTAMP WITH TIME ZONE NOT NULL,
    value TEXT NOT NULL
);
```

### MySQL/MariaDB
```sql
CREATE TABLE IF NOT EXISTS your_table_name (
    id INT AUTO_INCREMENT PRIMARY KEY,
    timestamp TIMESTAMP NOT NULL,
    value TEXT NOT NULL
);
```

---

## Serial Data Format

The application reads line-based data from the serial port. Each line should contain the data you want to store. 

---

## Error Handling

The application handles various error conditions gracefully:

- **Serial port disconnection**: Continues attempting to read
- **Database connection loss**: Logs errors and continues operation
- **Invalid data**: Skips malformed readings
- **Timeout conditions**: Non-blocking, continues operation

---

## Performance Considerations

- **Upload frequency**: Set to 2+ seconds to avoid overwhelming the database
- **Data deduplication**: Identical consecutive values are not re-uploaded
- **Asynchronous processing**: Serial reading and database operations run concurrently
- **Memory efficiency**: No dynamic allocation for data storage

---

## Troubleshooting

### Common Issues

**Serial port not found**
- Verify the device is connected and the port path is correct

**Database connection failed**
- Verify database credentials and network connectivity
- Ensure the database exists and user has proper permissions

**No data received**
- Check serial device configuration (baud rate, data format)
- Verify the device is sending line-terminated data

**Permission denied**
- On Linux, add your user to the `dialout` group for serial access
- Ensure database user has CREATE and INSERT permissions

---

## License

This project is licensed under the [MIT License](LICENSE).

---

## Author

**0xEthamin**

Feel free to open issues or contribute improvements.