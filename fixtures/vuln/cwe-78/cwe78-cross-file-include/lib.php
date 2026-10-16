<?php

function handle(string $value): void
{
    system('ls -l ' . $value);
}
