export default {
  id: "java-structural-drift",
  title: "Java service refactor — structural drift, no security flags",
  repo: {
    project: "orders-api",
    branch: "agent/refactor-orders",
  },
  before: {
    "src/OrderService.java": `class OrderService {
    Order place(Cart cart) {
        if (cart.isEmpty()) {
            return Order.none();
        }
        return checkout(cart);
    }
}
`,
  },
  after: {
    "src/OrderService.java": `class OrderService {
    Order place(Cart cart) {
        return checkout(cart);
    }

    void cancel(Order order) {
        refund(order);
    }
}
`,
  },
  oracle: {
    expectedExitCode: 0,
    changedFiles: 1,
    riskCount: 0,
    requiredFlags: [],
    forbiddenFlags: [{ severity: "high" }, { severity: "medium" }, { severity: "low" }],
  },
  agent: {
    expectedDecision: "approve",
  },
};
