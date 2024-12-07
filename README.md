# SPIT

SPIT is a high-performance mock server generator that automatically creates API endpoints based on OpenAPI/Swagger specifications. It's designed to help developers quickly set up mock APIs for testing and development purposes.

## Features

- 🚀 Automatic mock server generation from OpenAPI/Swagger specs
- 🔄 Support for both local files and remote URLs
- 🎯 Request validation against OpenAPI schema
- 🎨 Customizable response patterns
- ⏱️ Configurable response delays
- 📝 Request logging
- 🎮 Custom response headers
- 🔍 Path parameter validation

### Planned Features

- 📦 Support for multiple input sources:
  - Insomnia Collections
  - Postman Collections
  - PostgreSQL Database Schema
  - MongoDB Collections
  - GraphQL Schemas
  - Protocol Buffers (protobuf)
  - Custom API Documentation Formats
- 🔌 Plugin system for extending input sources
- 🎭 Multiple mock data strategies
- 📊 Response scenarios based on database state
- 🔄 Two-way sync with databases
- 📱 Web interface for managing mock servers

## Quick Start

SPIT can be used either as a CLI tool or as a library.

### CLI Usage

1. Start a mock server from a remote Swagger specification:

```bash
spit scan --url https://api.example.com/swagger.json --port 8080
```

2. Start a mock server from a local Swagger file:

```bash
spit file --path ./swagger.json --port 8080
```

### Configuration

SPIT supports YAML or JSON configuration files for customizing mock behavior:

```yaml
delay: 1000 # Global response delay in milliseconds
status_code: 200 # Default response status code
headers: # Custom response headers
  X-Custom-Header: "custom-value"
fields: # Custom field patterns
  patterns:
    cardNumber:
      type: "card"
      length: 16
    timestamp:
      type: "date"
      format: "%Y-%m-%d"
    price:
      type: "number"
      min: 10.0
      max: 1000.0
      decimals: 2
```

To use a configuration file:

```bash
spit scan --url https://api.example.com/swagger.json --config config.yaml
```

## Custom Field Patterns

SPIT supports several types of custom field patterns:

- **Enum**: Generate values from a predefined list
- **Number**: Generate numeric values within a range
- **CreditCard**: Generate credit card numbers
- **DateTime**: Generate formatted timestamps

Example configuration:

```yaml
fields:
  patterns:
    status:
      type: "enum"
      values: ["pending", "completed", "failed"]
    amount:
      type: "number"
      min: 0
      max: 1000
      decimals: 2
```

## Request Validation

SPIT automatically validates incoming requests against your OpenAPI schema:

- Path parameter validation
- Required header validation
- Request body schema validation
- Data type validation
- Required field validation

## Response Generation

Responses are automatically generated based on the OpenAPI schema definition:

- Follows response schema structure
- Generates realistic mock data
- Supports nested objects and arrays
- Handles references (`$ref`)
- Supports custom patterns for specific fields

## CLI Options

```
USAGE:
    spit <SUBCOMMAND>

SUBCOMMANDS:
    scan         Start server from remote Swagger URL
    file         Start server from local Swagger file
    insomnia     [Coming Soon] Start server from Insomnia Collection
    postman      [Coming Soon] Start server from Postman Collection
    postgres     [Coming Soon] Start server from PostgreSQL schema
    mongodb      [Coming Soon] Start server from MongoDB collections
    graphql      [Coming Soon] Start server from GraphQL schema
    protobuf     [Coming Soon] Start server from Protocol Buffers

OPTIONS:
    -p, --port <PORT>        Port to run the server on [default: 8080]
    -H, --host <HOST>        Host address to bind to [default: 127.0.0.1]
    -d, --delay <DELAY>      Global response delay in milliseconds
    -C, --config <CONFIG>    Path to configuration file
    -h, --help              Print help information
    -V, --version           Print version information
```

## Extending SPIT

SPIT is designed to be extensible through a plugin system (coming soon). You'll be able to create custom source providers by implementing the `SourceProvider` trait:

```rust
// Coming soon
trait SourceProvider {
    fn name(&self) -> &str;
    fn description(&self) -> &str;
    fn load(&self, source: &str) -> Result<ApiDefinition, Box<dyn Error>>;
}
```

This will allow SPIT to support any kind of input source while maintaining a consistent interface for mock server generation.

## Contributing

Contributions are welcome! Please feel free to submit a Pull Request. Areas we're particularly interested in:

- New input source providers
- Enhanced mock data generation strategies
- Database integration improvements
- Web interface development
- Performance optimizations
- Documentation and examples

## Author

Erick Jesus <erick.jesus2060@gmail.com>
