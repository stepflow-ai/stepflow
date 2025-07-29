#!/usr/bin/env python3
# Licensed to the Apache Software Foundation (ASF) under one or more contributor
# license agreements.  See the NOTICE file distributed with this work for
# additional information regarding copyright ownership.  The ASF licenses this
# file to you under the Apache License, Version 2.0 (the "License"); you may not
# use this file except in compliance with the License.  You may obtain a copy of
# the License at
#
#   http://www.apache.org/licenses/LICENSE-2.0
#
# Unless required by applicable law or agreed to in writing, software
# distributed under the License is distributed on an "AS IS" BASIS, WITHOUT
# WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.  See the
# License for the specific language governing permissions and limitations under
# the License.

"""
Example of using the StepFlow Python SDK to create a custom component server
with domain-specific business logic.

This demonstrates:
1. Using the SDK as a library
2. Creating typed business components
3. Using StepflowContext for blob operations
4. Handling both sync and async components
"""

from stepflow_py import StepflowStdioServer, StepflowContext
import msgspec
from typing import List, Optional
import asyncio

# Create the server
server = StepflowStdioServer()


# Business domain types
class Customer(msgspec.Struct):
    id: str
    name: str
    email: str
    tier: str  # "bronze", "silver", "gold"


class Order(msgspec.Struct):
    customer_id: str
    amount: float
    product: str


class CustomerAnalysisInput(msgspec.Struct):
    customers: List[Customer]
    orders: List[Order]


class CustomerAnalysisOutput(msgspec.Struct):
    total_revenue: float
    top_customers: List[Customer]
    tier_breakdown: dict


class ReportInput(msgspec.Struct):
    analysis: CustomerAnalysisOutput
    report_title: str


class ReportOutput(msgspec.Struct):
    report_blob_id: str
    summary: str


# Business logic components
@server.component
def analyze_customers(input: CustomerAnalysisInput) -> CustomerAnalysisOutput:
    """Analyze customer data and orders to generate insights."""

    # Calculate total revenue
    total_revenue = sum(order.amount for order in input.orders)

    # Find top customers by order volume
    customer_orders = {}
    for order in input.orders:
        customer_orders[order.customer_id] = (
            customer_orders.get(order.customer_id, 0) + order.amount
        )

    # Get top 3 customers
    top_customer_ids = sorted(
        customer_orders.keys(), key=lambda x: customer_orders[x], reverse=True
    )[:3]
    top_customers = [c for c in input.customers if c.id in top_customer_ids]

    # Tier breakdown
    tier_revenue = {"bronze": 0, "silver": 0, "gold": 0}
    customer_dict = {c.id: c for c in input.customers}

    for order in input.orders:
        customer = customer_dict.get(order.customer_id)
        if customer:
            tier_revenue[customer.tier] += order.amount

    return CustomerAnalysisOutput(
        total_revenue=total_revenue,
        top_customers=top_customers,
        tier_breakdown=tier_revenue,
    )


@server.component
async def generate_report(input: ReportInput, context: StepflowContext) -> ReportOutput:
    """Generate a detailed business report and store it as a blob."""

    analysis = input.analysis

    # Generate detailed report
    report = {
        "title": input.report_title,
        "total_revenue": analysis.total_revenue,
        "customer_analysis": {
            "top_customers": [
                {"name": c.name, "tier": c.tier, "email": c.email}
                for c in analysis.top_customers
            ],
            "tier_breakdown": analysis.tier_breakdown,
            "tier_percentages": {
                tier: (
                    (revenue / analysis.total_revenue * 100)
                    if analysis.total_revenue > 0
                    else 0
                )
                for tier, revenue in analysis.tier_breakdown.items()
            },
        },
        "recommendations": _generate_recommendations(analysis),
    }

    # Store report as blob
    report_blob_id = await context.put_blob(report)

    # Generate summary
    top_customer_names = [c.name for c in analysis.top_customers]
    summary = f"Generated {input.report_title} with ${analysis.total_revenue:,.2f} total revenue. Top customers: {', '.join(top_customer_names)}"

    return ReportOutput(report_blob_id=report_blob_id, summary=summary)


def _generate_recommendations(analysis: CustomerAnalysisOutput) -> List[str]:
    """Generate business recommendations based on analysis."""
    recommendations = []

    if analysis.tier_breakdown["gold"] / analysis.total_revenue < 0.3:
        recommendations.append("Consider upgrading more customers to Gold tier")

    if len(analysis.top_customers) < 3:
        recommendations.append("Focus on customer acquisition to diversify revenue")

    if analysis.tier_breakdown["bronze"] / analysis.total_revenue > 0.5:
        recommendations.append("Implement customer retention program for Bronze tier")

    return recommendations


if __name__ == "__main__":
    server.run()
