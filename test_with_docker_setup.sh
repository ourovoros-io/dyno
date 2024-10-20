#!/bin/bash

# This script sets up a PostgreSQL database using Docker for testing purposes.
# Do not use this script in production.

# Set variables
DB_CONTAINER_NAME="forc"
DB_USER="postgres"
DB_PASSWORD="forc"
DB_NAME="forc"
DB_PORT="5432"
POSTGRES_IMAGE="postgres:latest"

# Export variables
export DB_CONTAINER_NAME="forc"
export DB_USER="postgres"
export DB_PASSWORD="forc"
export DB_NAME="forc"
export DB_HOST="localhost"
export DB_PORT="5432"
export CERT="test_data/ca-certificate.crt"

# Check if Docker is installed
if ! command -v docker &> /dev/null
then
    echo "Docker is not installed. Please install Docker and try again."
    exit 1
fi

# Check if the PostgreSQL image is present
if ! docker image inspect $POSTGRES_IMAGE > /dev/null 2>&1
then
    echo "PostgreSQL image not found. Pulling the image..."
    docker pull $POSTGRES_IMAGE
fi

# Start PostgreSQL container
docker run --name $DB_CONTAINER_NAME -p $DB_PORT:$DB_PORT -e POSTGRES_USER=$DB_USER -e POSTGRES_PASSWORD=$DB_PASSWORD -e POSTGRES_DB=$DB_NAME -d $POSTGRES_IMAGE

# Wait for PostgreSQL to be ready
echo "Waiting for PostgreSQL to be ready..."
until docker exec $DB_CONTAINER_NAME pg_isready -U $DB_USER; do
  sleep 2
done

# Print message
echo "PostgreSQL is ready!"