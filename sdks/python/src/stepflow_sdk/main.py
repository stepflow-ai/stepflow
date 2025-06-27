from stepflow_sdk.server import StepflowStdioServer
from stepflow_sdk.udf import udf

# Create server instance
server = StepflowStdioServer()

# Register the UDF component
server.component(udf)

def main():
    # Start the server
    server.run()

if __name__ == "__main__":
    main()