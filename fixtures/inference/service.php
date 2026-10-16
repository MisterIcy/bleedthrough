<?php
namespace Billing;

interface Gateway { public function charge(int $cents): bool; }

abstract class BaseGateway implements Gateway
{
    abstract public function charge(int $cents): bool;
    protected function log(string $m): void {}
}

final class Stripe extends BaseGateway
{
    public function charge(int $cents): bool { $this->log('stripe'); return true; }
}

final class Paypal extends BaseGateway
{
    public function charge(int $cents): bool { return false; }
}

class Magic
{
    public function __call(string $n, array $a) { return null; }
}

class Invoice
{
    public function __construct(private Gateway $gateway, private Magic $magic) {}

    public function pay(int $cents): string
    {
        $ok = $this->gateway->charge($cents);
        $this->magic->anything();
        $total = $cents > 0 ? $cents : 0;
        $label = 'inv-' . $total;
        $format = fn(int $n): string => sprintf('%d', $n);
        $format($total);
        $fn = $ok ? 'strlen' : 'strtoupper';
        $fn($label);
        $make = [new Stripe(), 'charge'];
        $make(1);
        return $ok ? $label : (string) $total;
    }
}
