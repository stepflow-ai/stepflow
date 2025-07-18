---
sidebar_position: 4
---

# Data Processing

StepFlow excels at building robust data processing pipelines. These examples demonstrate ETL operations, data validation, transformation workflows, and integration with external data sources.

## ETL Pipeline Example

A comprehensive Extract, Transform, Load pipeline with validation and error handling.

:::info Future Components
This example uses several planned future components:
- `/database/query` - SQL database operations
- `/csv/parse` - CSV file parsing
- `/http/post` - HTTP POST requests
- `/validation/schema` - Data schema validation

Currently, you can achieve similar functionality using custom Python components with libraries like `pandas`, `sqlalchemy`, and `requests`.
:::

```yaml
name: "Sales Data ETL Pipeline"
description: "Extract sales data from multiple sources, transform, validate, and load to data warehouse"

input_schema:
  type: object
  properties:
    data_sources:
      type: object
      properties:
        sales_db_url: { type: string }
        customer_file_path: { type: string }
        product_api_url: { type: string }
      required: ["sales_db_url", "customer_file_path"]
    output_config:
      type: object
      properties:
        warehouse_url: { type: string }
        batch_size: { type: integer, default: 1000 }
        validation_level: { type: string, enum: ["strict", "standard", "relaxed"], default: "standard" }

steps:
  # EXTRACT PHASE - Run extractions in parallel

  # Extract sales data from database
  - id: extract_sales_data
    component: /database/query  # Future component
    on_error:
      action: continue
      default_output:
        data: []
        row_count: 0
        extraction_failed: true
    input:
      connection_string: { $from: { workflow: input }, path: "data_sources.sales_db_url" }
      query: |
        SELECT
          sale_id,
          customer_id,
          product_id,
          quantity,
          unit_price,
          sale_date,
          sales_rep_id
        FROM sales
        WHERE sale_date >= CURRENT_DATE - INTERVAL '30 days'
        ORDER BY sale_date DESC
      timeout: 30

  # Extract customer data from CSV file
  - id: extract_customer_data
    component: /csv/parse  # Future component
    on_error:
      action: continue
      default_output:
        data: []
        row_count: 0
        extraction_failed: true
    input:
      file_path: { $from: { workflow: input }, path: "data_sources.customer_file_path" }
      has_header: true
      delimiter: ","
      encoding: "utf-8"

  # Extract product data from API
  - id: extract_product_data
    component: /http/get  # Future component
    on_error:
      action: continue
      default_output:
        data: []
        status_code: 0
        extraction_failed: true
    input:
      url: { $from: { workflow: input }, path: "data_sources.product_api_url" }
      headers:
        Accept: "application/json"
        User-Agent: "StepFlow-ETL/1.0"
      timeout: 15

  # VALIDATION PHASE - Validate extracted data

  # Validate sales data structure
  - id: validate_sales_data
    component: /validation/schema  # Future component
    input:
      data: { $from: { step: extract_sales_data }, path: "data" }
      schema:
        type: array
        items:
          type: object
          properties:
            sale_id: { type: string }
            customer_id: { type: string }
            product_id: { type: string }
            quantity: { type: number, minimum: 0 }
            unit_price: { type: number, minimum: 0 }
            sale_date: { type: string, format: date }
            sales_rep_id: { type: string }
          required: ["sale_id", "customer_id", "product_id", "quantity", "unit_price", "sale_date"]

  # Create data quality report
  - id: create_quality_report
    component: /builtin/put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            sales_data: { type: object }
            customer_data: { type: object }
            product_data: { type: object }
            sales_validation: { type: object }
        code: |
          # Analyze data quality across all sources

          sales = input['sales_data']
          customers = input['customer_data']
          products = input['product_data']
          validation = input['sales_validation']

          report = {
            'extraction_summary': {
              'sales_records': sales.get('row_count', 0),
              'customer_records': customers.get('row_count', 0),
              'product_records': len(products.get('data', [])),
              'extraction_errors': [
                s for s in [sales, customers, products]
                if s.get('extraction_failed', False)
              ]
            },
            'data_quality': {
              'sales_validation_passed': validation.get('is_valid', False),
              'validation_errors': validation.get('errors', []),
              'completeness_score': 0.0,  # Would calculate based on null values
              'consistency_score': 0.0   # Would calculate based on referential integrity
            },
            'recommendations': []
          }

          # Add quality recommendations
          if sales.get('row_count', 0) == 0:
            report['recommendations'].append("Sales data extraction failed or returned no records")

          if not validation.get('is_valid', False):
            report['recommendations'].append("Sales data validation failed - check data structure")

          if len(report['extraction_summary']['extraction_errors']) > 0:
            report['recommendations'].append("Some data extractions failed - check source connectivity")

          # Calculate overall quality score
          sources_successful = 3 - len(report['extraction_summary']['extraction_errors'])
          validation_passed = 1 if validation.get('is_valid', False) else 0
          report['overall_quality_score'] = (sources_successful + validation_passed) / 4

          report

  # Execute quality report
  - id: execute_quality_report
    component: /python/udf
    input:
      blob_id: { $from: { step: create_quality_report }, path: "blob_id" }
      input:
        sales_data: { $from: { step: extract_sales_data } }
        customer_data: { $from: { step: extract_customer_data } }
        product_data: { $from: { step: extract_product_data } }
        sales_validation: { $from: { step: validate_sales_data } }

  # TRANSFORM PHASE - Clean and enrich data

  # Transform and enrich sales data
  - id: transform_sales_data
    component: /builtin/put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            sales: { type: array }
            customers: { type: array }
            products: { type: array }
        code: |
          import datetime
          from decimal import Decimal

          # Create lookup dictionaries for enrichment
          customer_lookup = {c['customer_id']: c for c in input['customers']}
          product_lookup = {p['product_id']: p for p in input['products']}

          transformed_sales = []

          for sale in input['sales']:
            # Basic transformations
            transformed_sale = {
              'sale_id': sale['sale_id'],
              'sale_date': sale['sale_date'],
              'quantity': float(sale['quantity']),
              'unit_price': float(sale['unit_price']),
              'total_amount': float(sale['quantity']) * float(sale['unit_price']),

              # Customer enrichment
              'customer_id': sale['customer_id'],
              'customer_name': customer_lookup.get(sale['customer_id'], {}).get('name', 'Unknown'),
              'customer_segment': customer_lookup.get(sale['customer_id'], {}).get('segment', 'Standard'),

              # Product enrichment
              'product_id': sale['product_id'],
              'product_name': product_lookup.get(sale['product_id'], {}).get('name', 'Unknown'),
              'product_category': product_lookup.get(sale['product_id'], {}).get('category', 'General'),
              'product_cost': product_lookup.get(sale['product_id'], {}).get('cost', 0.0),

              # Calculated fields
              'profit_margin': 0.0,  # Would calculate: (unit_price - product_cost) / unit_price
              'sales_rep_id': sale.get('sales_rep_id', 'UNKNOWN'),

              # Metadata
              'processed_at': datetime.datetime.now().isoformat(),
              'data_quality_score': 1.0  # Would calculate based on completeness
            }

            # Calculate profit margin if we have cost data
            product_cost = product_lookup.get(sale['product_id'], {}).get('cost', 0)
            if product_cost and transformed_sale['unit_price'] > 0:
              transformed_sale['profit_margin'] = (
                (transformed_sale['unit_price'] - product_cost) / transformed_sale['unit_price']
              )

            transformed_sales.append(transformed_sale)

          {
            'transformed_records': transformed_sales,
            'transformation_summary': {
              'records_processed': len(transformed_sales),
              'customers_matched': len([s for s in transformed_sales if s['customer_name'] != 'Unknown']),
              'products_matched': len([s for s in transformed_sales if s['product_name'] != 'Unknown']),
              'profit_margins_calculated': len([s for s in transformed_sales if s['profit_margin'] > 0])
            }
          }

  # Execute sales transformation
  - id: execute_sales_transformation
    component: /python/udf
    input:
      blob_id: { $from: { step: transform_sales_data }, path: "blob_id" }
      input:
        sales: { $from: { step: extract_sales_data }, path: "data" }
        customers: { $from: { step: extract_customer_data }, path: "data" }
        products: { $from: { step: extract_product_data }, path: "data" }

  # LOAD PHASE - Load to data warehouse

  # Prepare data for loading
  - id: prepare_warehouse_load
    component: /builtin/put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            transformed_data: { type: object }
            batch_size: { type: integer }
        code: |
          # Prepare data in batches for warehouse loading

          records = input['transformed_data']['transformed_records']
          batch_size = input['batch_size']

          # Split into batches
          batches = []
          for i in range(0, len(records), batch_size):
            batch = records[i:i + batch_size]
            batches.append({
              'batch_id': f"batch_{i // batch_size + 1}",
              'records': batch,
              'record_count': len(batch),
              'start_index': i,
              'end_index': i + len(batch) - 1
            })

          {
            'batches': batches,
            'total_batches': len(batches),
            'total_records': len(records),
            'load_strategy': 'batch_insert',
            'prepared_at': input['transformed_data'].get('transformation_summary', {})
          }

  # Execute load preparation
  - id: execute_load_preparation
    component: /python/udf
    input:
      blob_id: { $from: { step: prepare_warehouse_load }, path: "blob_id" }
      input:
        transformed_data: { $from: { step: execute_sales_transformation } }
        batch_size: { $from: { workflow: input }, path: "output_config.batch_size" }

  # Load data to warehouse
  - id: load_to_warehouse
    component: /database/bulk_insert  # Future component
    on_error:
      action: continue
      default_output:
        success: false
        records_loaded: 0
        error_message: "Warehouse load failed"
    input:
      connection_string: { $from: { workflow: input }, path: "output_config.warehouse_url" }
      table_name: "sales_fact"
      data: { $from: { step: execute_load_preparation }, path: "batches" }
      upsert_key: ["sale_id"]
      batch_size: { $from: { workflow: input }, path: "output_config.batch_size" }

  # Create final ETL report
  - id: create_etl_report
    component: /builtin/put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            quality_report: { type: object }
            transformation_result: { type: object }
            load_preparation: { type: object }
            warehouse_load: { type: object }
        code: |
          import datetime

          # Create comprehensive ETL execution report
          report = {
            'etl_metadata': {
              'pipeline_name': 'Sales Data ETL',
              'execution_start': datetime.datetime.now().isoformat(),
              'execution_id': f"etl_{datetime.datetime.now().strftime('%Y%m%d_%H%M%S')}",
              'pipeline_version': '1.0'
            },
            'extraction_phase': {
              'sources_attempted': 3,
              'sources_successful': 3 - len(input['quality_report']['extraction_summary']['extraction_errors']),
              'total_records_extracted': (
                input['quality_report']['extraction_summary']['sales_records'] +
                input['quality_report']['extraction_summary']['customer_records'] +
                input['quality_report']['extraction_summary']['product_records']
              ),
              'extraction_errors': input['quality_report']['extraction_summary']['extraction_errors']
            },
            'transformation_phase': {
              'records_transformed': input['transformation_result']['transformation_summary']['records_processed'],
              'enrichment_success': {
                'customers_matched': input['transformation_result']['transformation_summary']['customers_matched'],
                'products_matched': input['transformation_result']['transformation_summary']['products_matched'],
                'profit_margins_calculated': input['transformation_result']['transformation_summary']['profit_margins_calculated']
              }
            },
            'load_phase': {
              'batches_prepared': input['load_preparation']['total_batches'],
              'records_prepared': input['load_preparation']['total_records'],
              'warehouse_load_success': input['warehouse_load'].get('success', False),
              'records_loaded': input['warehouse_load'].get('records_loaded', 0)
            },
            'data_quality': input['quality_report']['data_quality'],
            'overall_success': (
              input['quality_report']['overall_quality_score'] > 0.7 and
              input['warehouse_load'].get('success', False)
            )
          }

          # Add recommendations
          report['recommendations'] = input['quality_report']['recommendations'].copy()

          if not input['warehouse_load'].get('success', False):
            report['recommendations'].append("Warehouse load failed - investigate connectivity and permissions")

          if report['transformation_phase']['enrichment_success']['customers_matched'] < report['transformation_phase']['records_transformed'] * 0.9:
            report['recommendations'].append("Low customer match rate - verify customer data completeness")

          report

  # Execute ETL report creation
  - id: execute_etl_report
    component: /python/udf
    input:
      blob_id: { $from: { step: create_etl_report }, path: "blob_id" }
      input:
        quality_report: { $from: { step: execute_quality_report } }
        transformation_result: { $from: { step: execute_sales_transformation } }
        load_preparation: { $from: { step: execute_load_preparation } }
        warehouse_load: { $from: { step: load_to_warehouse } }

output:
  etl_report: { $from: { step: execute_etl_report } }

test:
  cases:
  - name: successful_etl_run
    description: Complete ETL pipeline with all sources available
    input:
      data_sources:
        sales_db_url: "postgresql://localhost/sales_db"
        customer_file_path: "customers.csv"
        product_api_url: "https://api.example.com/products"
      output_config:
        warehouse_url: "postgresql://localhost/warehouse"
        batch_size: 500
        validation_level: "standard"
```

## Real-time Data Processing

:::info Future Components
This example demonstrates a real-time processing pattern using planned components:
- `/stream/kafka_consumer` - Kafka message consumption
- `/cache/redis` - Redis caching operations
- `/metrics/publish` - Metrics publication

These can be implemented today using custom Python components with appropriate libraries.
:::

```yaml
name: "Real-time Event Processing"
description: "Process streaming events with real-time analytics and alerting"

input_schema:
  type: object
  properties:
    stream_config:
      type: object
      properties:
        kafka_bootstrap_servers: { type: string }
        topic_name: { type: string }
        consumer_group: { type: string }
        batch_size: { type: integer, default: 100 }
    processing_rules:
      type: object
      properties:
        alert_thresholds: { type: object }
        aggregation_window_minutes: { type: integer, default: 5 }
        enable_real_time_alerts: { type: boolean, default: true }

steps:
  # Consume events from Kafka stream
  - id: consume_events
    component: /stream/kafka_consumer  # Future component
    input:
      bootstrap_servers: { $from: { workflow: input }, path: "stream_config.kafka_bootstrap_servers" }
      topic: { $from: { workflow: input }, path: "stream_config.topic_name" }
      group_id: { $from: { workflow: input }, path: "stream_config.consumer_group" }
      auto_offset_reset: "latest"
      max_poll_records: { $from: { workflow: input }, path: "stream_config.batch_size" }

  # Parse and validate incoming events
  - id: parse_events
    component: /builtin/put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            raw_events: { type: array }
        code: |
          import json
          import datetime

          parsed_events = []
          parse_errors = []

          for event in input['raw_events']:
            try:
              # Parse JSON event
              if isinstance(event, str):
                event_data = json.loads(event)
              else:
                event_data = event

              # Validate required fields
              required_fields = ['event_id', 'event_type', 'timestamp', 'user_id']
              missing_fields = [f for f in required_fields if f not in event_data]

              if missing_fields:
                parse_errors.append({
                  'raw_event': event,
                  'error': f'Missing fields: {missing_fields}'
                })
                continue

              # Enrich event
              enriched_event = {
                'event_id': event_data['event_id'],
                'event_type': event_data['event_type'],
                'user_id': event_data['user_id'],
                'timestamp': event_data['timestamp'],
                'properties': event_data.get('properties', {}),
                'processed_at': datetime.datetime.now().isoformat(),
                'processing_batch': f"batch_{datetime.datetime.now().strftime('%Y%m%d_%H%M%S')}"
              }

              parsed_events.append(enriched_event)

            except Exception as e:
              parse_errors.append({
                'raw_event': event,
                'error': str(e)
              })

          {
            'parsed_events': parsed_events,
            'parse_errors': parse_errors,
            'success_rate': len(parsed_events) / len(input['raw_events']) if input['raw_events'] else 0,
            'total_processed': len(input['raw_events'])
          }

  # Execute event parsing
  - id: execute_event_parsing
    component: /python/udf
    input:
      blob_id: { $from: { step: parse_events }, path: "blob_id" }
      input:
        raw_events: { $from: { step: consume_events }, path: "events" }

  # Real-time analytics and aggregation
  - id: calculate_real_time_metrics
    component: /builtin/put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            events: { type: array }
            window_minutes: { type: integer }
        code: |
          import datetime
          from collections import Counter, defaultdict

          events = input['events']
          window_minutes = input['window_minutes']

          # Calculate current window
          now = datetime.datetime.now()
          window_start = now - datetime.timedelta(minutes=window_minutes)

          # Aggregate metrics
          event_counts = Counter()
          user_activity = defaultdict(int)
          error_events = []

          for event in events:
            # Count by event type
            event_counts[event['event_type']] += 1

            # Track user activity
            user_activity[event['user_id']] += 1

            # Identify potential errors or anomalies
            if event['event_type'] in ['error', 'exception', 'failure']:
              error_events.append(event)

          # Calculate rates and trends
          total_events = len(events)
          unique_users = len(user_activity)
          error_rate = len(error_events) / total_events if total_events > 0 else 0

          metrics = {
            'window_metrics': {
              'window_start': window_start.isoformat(),
              'window_end': now.isoformat(),
              'total_events': total_events,
              'unique_users': unique_users,
              'events_per_minute': total_events / window_minutes if window_minutes > 0 else 0,
              'error_rate': error_rate
            },
            'event_type_breakdown': dict(event_counts),
            'top_active_users': sorted(
              user_activity.items(),
              key=lambda x: x[1],
              reverse=True
            )[:10],
            'error_events': error_events[:10],  # Keep sample of errors
            'alerts': []
          }

          # Generate alerts based on thresholds
          if error_rate > 0.05:  # 5% error rate threshold
            metrics['alerts'].append({
              'type': 'high_error_rate',
              'message': f'Error rate {error_rate:.2%} exceeds 5% threshold',
              'severity': 'warning'
            })

          if total_events > 1000:  # High volume threshold
            metrics['alerts'].append({
              'type': 'high_volume',
              'message': f'High event volume: {total_events} events in {window_minutes} minutes',
              'severity': 'info'
            })

          metrics

  # Execute real-time analytics
  - id: execute_real_time_analytics
    component: /python/udf
    input:
      blob_id: { $from: { step: calculate_real_time_metrics }, path: "blob_id" }
      input:
        events: { $from: { step: execute_event_parsing }, path: "parsed_events" }
        window_minutes: { $from: { workflow: input }, path: "processing_rules.aggregation_window_minutes" }

  # Cache metrics for dashboard
  - id: cache_metrics
    component: /cache/redis  # Future component
    on_error:
      action: continue
      default_output:
        cached: false
        error: "Redis cache unavailable"
    input:
      operation: "set"
      key: "real_time_metrics"
      value: { $from: { step: execute_real_time_analytics } }
      ttl: 300  # 5 minutes

  # Publish metrics to monitoring system
  - id: publish_metrics
    component: /metrics/publish  # Future component
    input:
      metrics:
        - name: "events_processed_total"
          value: { $from: { step: execute_real_time_analytics }, path: "window_metrics.total_events" }
          tags:
            pipeline: "real_time_processing"
        - name: "unique_users_active"
          value: { $from: { step: execute_real_time_analytics }, path: "window_metrics.unique_users" }
          tags:
            pipeline: "real_time_processing"
        - name: "error_rate_percentage"
          value: { $from: { step: execute_real_time_analytics }, path: "window_metrics.error_rate" }
          tags:
            pipeline: "real_time_processing"

  # Send alerts if enabled
  - id: send_alerts
    component: /notifications/slack  # Future component
    skip:
      # Skip if no alerts or alerts disabled
      # This would check: !input.processing_rules.enable_real_time_alerts OR len(alerts) == 0
    input:
      webhook_url: "https://hooks.slack.com/services/YOUR/SLACK/WEBHOOK"
      message: |
        🚨 Real-time Processing Alerts:
        {{ $from: { step: execute_real_time_analytics }, path: "alerts" }}

        Metrics Summary:
        - Events: {{ $from: { step: execute_real_time_analytics }, path: "window_metrics.total_events" }}
        - Users: {{ $from: { step: execute_real_time_analytics }, path: "window_metrics.unique_users" }}
        - Error Rate: {{ $from: { step: execute_real_time_analytics }, path: "window_metrics.error_rate" }}

output:
  processing_result:
    parsed_events: { $from: { step: execute_event_parsing } }
    metrics: { $from: { step: execute_real_time_analytics } }
    cache_status: { $from: { step: cache_metrics } }
    alerts_sent: { $from: { step: send_alerts } }
```

## Data Quality Monitoring

A workflow that continuously monitors data quality across multiple datasets.

```yaml
name: "Data Quality Monitoring"
description: "Monitor data quality metrics and detect anomalies across datasets"

input_schema:
  type: object
  properties:
    datasets:
      type: array
      items:
        type: object
        properties:
          name: { type: string }
          source_type: { type: string, enum: ["database", "file", "api"] }
          connection_info: { type: object }
          quality_rules: { type: array }
        required: ["name", "source_type", "connection_info"]
    monitoring_config:
      type: object
      properties:
        check_frequency_minutes: { type: integer, default: 15 }
        alert_thresholds: { type: object }
        historical_comparison_days: { type: integer, default: 7 }

steps:
  # Profile each dataset in parallel
  - id: profile_dataset_1
    component: /builtin/put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            dataset: { type: object }
        code: |
          # Simulate data profiling for first dataset
          dataset = input['dataset']

          # This would connect to actual data source and analyze
          profile = {
            'dataset_name': dataset['name'],
            'total_records': 15000,  # Simulated
            'null_percentages': {
              'customer_id': 0.0,
              'order_date': 0.02,
              'product_id': 0.01,
              'quantity': 0.05,
              'amount': 0.03
            },
            'data_types': {
              'customer_id': 'string',
              'order_date': 'date',
              'product_id': 'string',
              'quantity': 'integer',
              'amount': 'decimal'
            },
            'value_distributions': {
              'quantity': {'min': 1, 'max': 100, 'avg': 5.2, 'std': 8.1},
              'amount': {'min': 0.99, 'max': 9999.99, 'avg': 156.78, 'std': 245.33}
            },
            'uniqueness': {
              'customer_id': 0.85,  # 85% unique values
              'product_id': 0.95,
              'order_date': 0.45
            }
          }

          profile

  # Execute dataset profiling for first dataset
  - id: execute_dataset_1_profiling
    component: /python/udf
    input:
      blob_id: { $from: { step: profile_dataset_1 }, path: "blob_id" }
      input:
        dataset: { $from: { workflow: input }, path: "datasets[0]" }

  # Apply quality rules to dataset
  - id: apply_quality_rules
    component: /builtin/put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            profile: { type: object }
            rules: { type: array }
        code: |
          import datetime

          profile = input['profile']
          rules = input.get('rules', [])

          quality_results = {
            'dataset_name': profile['dataset_name'],
            'check_timestamp': datetime.datetime.now().isoformat(),
            'rule_results': [],
            'overall_score': 0.0,
            'issues_found': []
          }

          # Default quality rules if none specified
          if not rules:
            rules = [
              {'type': 'null_percentage', 'field': 'customer_id', 'threshold': 0.01, 'operator': 'less_than'},
              {'type': 'null_percentage', 'field': 'amount', 'threshold': 0.05, 'operator': 'less_than'},
              {'type': 'uniqueness', 'field': 'customer_id', 'threshold': 0.80, 'operator': 'greater_than'},
              {'type': 'record_count', 'threshold': 1000, 'operator': 'greater_than'}
            ]

          passed_rules = 0

          for rule in rules:
            rule_result = {
              'rule_type': rule['type'],
              'field': rule.get('field'),
              'threshold': rule['threshold'],
              'operator': rule['operator'],
              'passed': False,
              'actual_value': None,
              'message': ''
            }

            if rule['type'] == 'null_percentage' and rule.get('field'):
              actual = profile['null_percentages'].get(rule['field'], 0)
              rule_result['actual_value'] = actual

              if rule['operator'] == 'less_than':
                rule_result['passed'] = actual < rule['threshold']

              if not rule_result['passed']:
                quality_results['issues_found'].append(
                  f"High null percentage in {rule['field']}: {actual:.2%}"
                )

            elif rule['type'] == 'uniqueness' and rule.get('field'):
              actual = profile['uniqueness'].get(rule['field'], 0)
              rule_result['actual_value'] = actual

              if rule['operator'] == 'greater_than':
                rule_result['passed'] = actual > rule['threshold']

              if not rule_result['passed']:
                quality_results['issues_found'].append(
                  f"Low uniqueness in {rule['field']}: {actual:.2%}"
                )

            elif rule['type'] == 'record_count':
              actual = profile['total_records']
              rule_result['actual_value'] = actual

              if rule['operator'] == 'greater_than':
                rule_result['passed'] = actual > rule['threshold']

              if not rule_result['passed']:
                quality_results['issues_found'].append(
                  f"Low record count: {actual}"
                )

            if rule_result['passed']:
              passed_rules += 1

            quality_results['rule_results'].append(rule_result)

          # Calculate overall quality score
          quality_results['overall_score'] = passed_rules / len(rules) if rules else 1.0
          quality_results['quality_grade'] = (
            'A' if quality_results['overall_score'] >= 0.9 else
            'B' if quality_results['overall_score'] >= 0.8 else
            'C' if quality_results['overall_score'] >= 0.7 else
            'D' if quality_results['overall_score'] >= 0.6 else 'F'
          )

          quality_results

  # Execute quality rule application
  - id: execute_quality_rules
    component: /python/udf
    input:
      blob_id: { $from: { step: apply_quality_rules }, path: "blob_id" }
      input:
        profile: { $from: { step: execute_dataset_1_profiling } }
        rules: { $from: { workflow: input }, path: "datasets[0].quality_rules" }

  # Generate quality dashboard data
  - id: generate_dashboard_data
    component: /builtin/put_blob
    input:
      data:
        input_schema:
          type: object
          properties:
            quality_results: { type: object }
            monitoring_config: { type: object }
        code: |
          import datetime

          quality = input['quality_results']
          config = input['monitoring_config']

          dashboard = {
            'summary': {
              'total_datasets': 1,  # Would be dynamic based on input
              'datasets_monitored': 1,
              'overall_quality_score': quality['overall_score'],
              'datasets_with_issues': 1 if quality['issues_found'] else 0,
              'last_check': quality['check_timestamp']
            },
            'dataset_scores': [
              {
                'name': quality['dataset_name'],
                'score': quality['overall_score'],
                'grade': quality['quality_grade'],
                'issues_count': len(quality['issues_found']),
                'status': 'healthy' if quality['overall_score'] >= 0.8 else 'warning' if quality['overall_score'] >= 0.6 else 'critical'
              }
            ],
            'recent_issues': quality['issues_found'],
            'monitoring_status': {
              'check_frequency': f"Every {config.get('check_frequency_minutes', 15)} minutes",
              'alert_thresholds_configured': bool(config.get('alert_thresholds')),
              'historical_comparison_enabled': config.get('historical_comparison_days', 0) > 0
            },
            'recommendations': []
          }

          # Add recommendations based on quality results
          if quality['overall_score'] < 0.8:
            dashboard['recommendations'].append("Review data quality rules and investigate failing checks")

          if len(quality['issues_found']) > 0:
            dashboard['recommendations'].append("Address data quality issues to improve overall score")

          if quality['overall_score'] < 0.6:
            dashboard['recommendations'].append("Critical data quality issues detected - immediate attention required")

          dashboard

  # Execute dashboard generation
  - id: execute_dashboard_generation
    component: /python/udf
    input:
      blob_id: { $from: { step: generate_dashboard_data }, path: "blob_id" }
      input:
        quality_results: { $from: { step: execute_quality_rules } }
        monitoring_config: { $from: { workflow: input }, path: "monitoring_config" }

output:
  quality_report: { $from: { step: execute_quality_rules } }
  dashboard_data: { $from: { step: execute_dashboard_generation } }

test:
  cases:
  - name: single_dataset_monitoring
    description: Monitor quality of a single dataset
    input:
      datasets:
        - name: "orders_dataset"
          source_type: "database"
          connection_info:
            host: "localhost"
            database: "sales"
            table: "orders"
          quality_rules:
            - type: "null_percentage"
              field: "customer_id"
              threshold: 0.01
              operator: "less_than"
            - type: "record_count"
              threshold: 1000
              operator: "greater_than"
      monitoring_config:
        check_frequency_minutes: 15
        historical_comparison_days: 7
```

:::tip Community Opportunity
Data quality monitoring components would be valuable additions to the StepFlow ecosystem. Consider implementing components for:
- Database profiling with libraries like `pandas-profiling` or `great-expectations`
- Data drift detection using statistical methods
- Automated data lineage tracking
- Integration with data catalogs and metadata systems
:::

## Next Steps

These data processing examples demonstrate StepFlow's capabilities for:

- **ETL Pipelines**: Robust extract, transform, load operations
- **Real-time Processing**: Stream processing with analytics and alerting
- **Quality Monitoring**: Continuous data quality assessment and reporting
- **Error Handling**: Graceful handling of data processing failures

Explore related patterns:
- **[AI Workflows](./ai-workflows.md)** - Combine AI with data processing
- **[Custom Components](./custom-components.md)** - Build specialized data processing components